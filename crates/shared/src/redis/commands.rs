/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::unwrap_used)]

use crate::redis::error::RedisError;
use crate::redis::types::*;
use fred::{
    interfaces::{
        GeoInterface, HashesInterface, KeysInterface, SortedSetsInterface, StreamsInterface,
    },
    prelude::ListInterface,
    types::{
        Expiration, FromRedis, GeoPosition, GeoRadiusInfo, GeoUnit, GeoValue, Limit,
        MultipleGeoValues, MultipleKeys, Ordering, RedisKey, RedisMap, RedisValue, SetOptions,
        SortOrder, StringOrNumber, XCapKind, XCapTrim, ZSort,
        XID::{self, Auto, Manual},
    },
};
use rustc_hash::FxHashMap;
use serde::{de::DeserializeOwned, Serialize};
use std::{fmt::Debug, ops::Deref};
use tracing::error;

impl RedisConnectionPool {
    /// Asynchronously sets a key-value pair in a Redis datastore with an expiry time.
    ///
    /// This function allows for setting a key with a specified value and an expiration time in Redis.
    /// It leverages the `fred` crate to interact with the Redis datastore. The function is generic
    /// over the value type, allowing different types that can be converted into a `RedisValue`.
    ///
    /// # Type Parameters
    /// * `V` - The type of the value to be set in the datastore. The type must implement the `TryInto<RedisValue>` trait.
    ///
    /// # Arguments
    /// * `key` - A reference to the string representing the key to be set in the datastore.
    /// * `value` - The value to be associated with the key. It is generic and can be any type that implements `TryInto<RedisValue>`.
    /// * `expiry` - The expiration time of the key-value pair, specified in seconds.
    ///
    /// # Returns
    /// * `Result<(), RedisError>` - Returns an `Ok(())` if the key-value pair is successfully set,
    ///   or an `Err(RedisError::SetFailed)` containing an error message if the operation fails.
    ///
    /// # Errors
    /// This function will return an error:
    /// * If there is a failure in setting the value associated with the key in Redis.
    /// * If the value type `V` fails to convert into `RedisValue`.
    pub async fn set_key<V>(&self, key: &str, value: V, expiry: u32) -> Result<(), RedisError>
    where
        V: Serialize + Send + Sync,
    {
        let serialized_value = serde_json::to_string(&value)
            .map_err(|err| RedisError::SerializationError(err.to_string()))?;

        let redis_value: RedisValue = serialized_value.into();

        self.pool
            .set(
                key,
                redis_value,
                Some(Expiration::EX(expiry.into())),
                None,
                false,
            )
            .await
            .map_err(|err| RedisError::SetFailed(err.to_string()))
    }

    pub async fn set_key_as_str(
        &self,
        key: &str,
        value: &str,
        expiry: u32,
    ) -> Result<(), RedisError> {
        let redis_value: RedisValue = value.into();
        self.pool
            .set(
                key,
                redis_value,
                Some(Expiration::EX(expiry.into())),
                None,
                false,
            )
            .await
            .map_err(|err| RedisError::SetFailed(err.to_string()))
    }

    /// Asynchronously sets a key-value pair in a Redis datastore with an expiry time, only if the key does not already exist.
    ///
    /// This function aims to perform a conditional set operation (SETNX) followed by setting an expiration time on the key.
    /// It uses a pipeline to combine the `SETNX` and `EXPIRE` commands for atomic execution. The function is generic
    /// over the value type, allowing different types that can be converted into a `RedisValue`.
    ///
    /// # Type Parameters
    /// * `V` - The type of the value to be set in the datastore. The type must implement the `TryInto<RedisValue>` trait.
    ///
    /// # Arguments
    /// * `key` - A reference to the string representing the key to be set in the datastore.
    /// * `value` - The value to be associated with the key. It is generic and can be any type that implements `TryInto<RedisValue>`.
    /// * `expiry` - The expiration time of the key-value pair, specified in seconds.
    ///
    /// # Returns
    /// * `Result<bool, RedisError>` - Returns an `Ok(true)` if the key-value pair is successfully set and the expiration time is applied.
    ///   Returns an `Ok(false)` if the key-value pair is already existing and hence not set.
    ///   Returns an `Err(RedisError::SetExFailed)` containing an error message if the operation fails.
    ///
    /// # Errors
    /// This function will return an error:
    /// * If there is a failure in setting the value associated with the key or applying the expiration time in Redis.
    /// * If the value type `V` fails to convert into `RedisValue`.
    /// * If an unexpected case is encountered during the operation.
    pub async fn setnx_with_expiry<V>(
        &self,
        key: &str,
        value: V,
        expiry: i64,
    ) -> Result<bool, RedisError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let pipeline = self.pool.pipeline();

        let _ = pipeline.msetnx::<RedisValue, _>((key, value)).await;
        let _ = pipeline.expire::<(), &str>(key, expiry).await;

        let output: Vec<RedisValue> = pipeline
            .all()
            .await
            .map_err(|err| RedisError::SetExFailed(err.to_string()))?;

        match output.deref() {
            [RedisValue::Integer(1), ..] => Ok(true),
            [RedisValue::Integer(0), ..] => Ok(false),
            case => Err(RedisError::SetExFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Asynchronously sets an expiration time for a given key in a Redis datastore.
    ///
    /// This function applies an expiration time to a specified key, causing the key to be
    /// automatically deleted after the given number of seconds. If the key is not present in the datastore,
    /// the function will still complete successfully, having no effect.
    ///
    /// # Arguments
    /// * `key` - A reference to a string representing the key to which the expiration time will be applied.
    /// * `seconds` - The expiration time in seconds. The key will be removed after this duration.
    ///
    /// # Returns
    /// * `Result<(), RedisError>` - Returns an `Ok(())` if the expiration time is successfully set.
    ///   Returns an `Err(RedisError::SetExpiryFailed)` containing an error message if the operation fails.
    ///
    /// # Errors
    /// This function will return an error if there is a failure in applying the expiration time to the key in Redis.
    pub async fn set_expiry(&self, key: &str, seconds: i64) -> Result<(), RedisError> {
        let output: Result<(), _> = self.pool.expire(key, seconds).await;

        if let Err(err) = output {
            Err(RedisError::SetExpiryFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    /// Asynchronously retrieves the value associated with a specified key in a Redis datastore.
    ///
    /// This function attempts to fetch the value of a specified key from a Redis datastore.
    /// It handles different cases based on the returned RedisValue. If a string is returned,
    /// it's converted and wrapped into an Option. If a null value is returned, an Option::None is returned.
    /// Errors and unexpected values result in a custom `RedisError`.
    ///
    /// # Arguments
    /// * `key` - A reference to a string representing the key whose value is to be fetched.
    ///
    /// # Returns
    /// * `Result<Option<T>, RedisError>` - Returns an `Ok(Some(T))` containing the deserialized
    ///   representation of the value associated with the key, an `Ok(None)` if the key is not present,
    ///   or an `Err(RedisError::GetFailed)` with an error message if the operation fails.
    ///
    /// # Errors
    /// This function will return an error if there is a failure in retrieving the value associated with the key from Redis.
    pub async fn get_key<T>(&self, key: &str) -> Result<Option<T>, RedisError>
    where
        T: DeserializeOwned,
    {
        let output: RedisValue = self
            .pool
            .get(key)
            .await
            .map_err(|err| RedisError::GetFailed(err.to_string()))?;

        match output {
            RedisValue::String(val) => serde_json::from_str(&val)
                .map(Some)
                .map_err(|err| RedisError::DeserializationError(err.to_string())),
            RedisValue::Null => Ok(None),
            case => Err(RedisError::GetFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Retrieves a value associated with the given key from Redis as a string.
    ///
    /// This function queries Redis for the key and attempts to return the value as a string.
    /// If the key does not exist or the value is not a string, appropriate errors are returned.
    ///
    /// # Arguments
    /// * `key` - A string slice that holds the key to retrieve the value for.
    ///
    /// # Returns
    /// This function returns a `Result` which is:
    /// - `Ok(Some(String))` if the key exists and the value is a string.
    /// - `Ok(None)` if the key does not exist in Redis.
    /// - `Err(RedisError)` if there is a problem retrieving the value or the value is not a string.
    ///
    /// # Errors
    /// This function will return an `RedisError::GetFailed` error in the following cases:
    /// - If the Redis query itself fails for any reason (e.g., connection issues).
    /// - If the value retrieved is not a string or is another data type not expected.
    pub async fn get_key_as_str(&self, key: &str) -> Result<Option<String>, RedisError> {
        let output: RedisValue = self
            .pool
            .get(key)
            .await
            .map_err(|err| RedisError::GetFailed(err.to_string()))?;

        match output {
            RedisValue::String(val) => Ok(Some(val.to_string())),
            RedisValue::Null => Ok(None),
            case => Err(RedisError::GetFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Asynchronously retrieves the values associated with multiple keys in a Redis datastore.
    ///
    /// This function attempts to fetch the values of multiple keys from a Redis datastore simultaneously.
    /// An array of keys is passed as an argument, and a vector of `Option<String>` is returned,
    /// where each element represents the value of the corresponding key in the input vector.
    ///
    /// If the retrieved RedisValue is an array, it gets converted to a vector of `Option<String>`.
    /// If it's a single string or null value, a vector containing a single `Option<String>` is returned.
    /// Errors and unexpected values result in a custom `RedisError`.
    ///
    /// # Arguments
    /// * `keys` - A vector of strings where each string represents a key in the Redis datastore.
    ///
    /// # Returns
    /// * `Result<Vec<Option<String>>, RedisError>` - Returns an `Ok(Vec<Option<String>>)` containing
    ///   the string representations of the values associated with each key, or an
    ///   `Err(RedisError::MGetFailed)` with an error message if the operation fails.
    ///
    /// # Errors
    /// This function will return an error if there is a failure in retrieving the values associated with the keys from Redis.
    pub async fn mget_keys<T>(&self, keys: Vec<String>) -> Result<Vec<Option<T>>, RedisError>
    where
        T: DeserializeOwned,
    {
        if keys.is_empty() {
            return Ok(vec![]);
        }

        let keys: Vec<RedisKey> = keys.into_iter().map(RedisKey::from).collect();

        let output: RedisValue = self
            .pool
            .mget(MultipleKeys::from(keys))
            .await
            .map_err(|err| RedisError::MGetFailed(err.to_string()))?;

        match output {
            RedisValue::Array(val) => {
                let results = val
                    .into_iter()
                    .map(|v| match v {
                        RedisValue::String(s) => serde_json::from_str::<T>(&s)
                            .map(Some)
                            .map_err(|err| RedisError::DeserializationError(err.to_string())),
                        RedisValue::Null => Ok(None),
                        case => Err(RedisError::MGetFailed(format!(
                            "Unexpected RedisValue encountered : {:?}",
                            case
                        ))),
                    })
                    .collect::<Result<Vec<Option<T>>, RedisError>>()?;
                Ok(results)
            }
            RedisValue::String(val) => serde_json::from_str::<T>(&val)
                .map(|val| Some(val))
                .map_err(|err| RedisError::DeserializationError(err.to_string()))
                .map(|res| vec![res]),
            RedisValue::Null => Ok(vec![None]),
            case => Err(RedisError::MGetFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Deletes a key in the Redis store.
    ///
    /// Given a key, this asynchronous function will attempt to delete it from the Redis store.
    /// In case of success, it will return an empty `Result`. In case of failure, it will return
    /// an `RedisError::DeleteFailed` variant containing a description of the error.
    ///
    /// # Parameters
    /// - `key: &str` - The key to be deleted from the Redis store.
    ///
    /// # Returns
    /// - `Result<(), RedisError>` - An empty `Result` in case of success or an `RedisError::DeleteFailed` in case of failure.
    ///
    /// # Examples
    /// ```
    /// let result = your_redis_instance.delete_key("your_key").await;
    /// match result {
    ///     Ok(_) => println!("Key deleted successfully!"),
    ///     Err(e) => println!("An error occurred: {:?}", e),
    /// }
    /// ```
    pub async fn delete_key(&self, key: &str) -> Result<(), RedisError> {
        self.pool
            .del(key)
            .await
            .map_err(|err| RedisError::DeleteFailed(err.to_string()))
    }

    /// Deletes multiple keys in the Redis store as a part of a single pipeline.
    ///
    /// This asynchronous function receives a vector of keys and attempts to delete them all
    /// from the Redis store in a single pipeline operation. It returns an empty `Result` if all keys
    /// are successfully deleted, or an `RedisError::DeleteFailed` containing a description of the error if any failure occurs.
    ///
    /// # Parameters
    /// - `keys: Vec<&str>` - A vector containing the keys to be deleted from the Redis store.
    ///
    /// # Returns
    /// - `Result<(), RedisError>` - An empty `Result` on successful deletion of all keys, or an `RedisError::DeleteFailed` on failure.
    ///
    /// # Examples
    /// ```
    /// let keys_to_delete = vec!["key1", "key2", "key3"];
    /// let result = your_redis_instance.delete_keys(keys_to_delete).await;
    /// match result {
    ///     Ok(_) => println!("Keys deleted successfully!"),
    ///     Err(e) => println!("An error occurred: {:?}", e),
    /// }
    /// ```
    pub async fn delete_keys(&self, keys: Vec<&str>) -> Result<(), RedisError> {
        let pipeline = self.pool.pipeline();

        for key in keys {
            let _ = pipeline.del::<RedisValue, &str>(key).await;
        }

        pipeline
            .all()
            .await
            .map_err(|err| RedisError::DeleteFailed(err.to_string()))?;

        Ok(())
    }

    /// Sets multiple fields in a hash in the Redis store and applies an expiry time to the hash.
    ///
    /// This asynchronous function receives a key representing a hash, a value representing field-value
    /// pairs to be set within the hash, and an expiry time. It attempts to set the specified fields
    /// in the hash and apply an expiry time to the entire hash.
    /// Returns a `Result` indicating the success or failure of the operation.
    ///
    /// # Type Parameters
    /// - `V` - The type representing the field-value pairs to be set within the hash.
    ///   Must be convertible into a `RedisMap` and implements `Debug`, `Send`, and `Sync`.
    ///
    /// # Parameters
    /// - `key: &str` - The key representing the hash in the Redis store.
    /// - `values: V` - The values representing the field-value pairs to be set within the hash.
    /// - `expiry: i64` - The expiry time to be applied to the hash, in seconds.
    ///
    /// # Returns
    /// - `Result<(), RedisError>` - A `Result` indicating the success (`Ok`) or failure (`Err`) of the operation.
    ///   Returns an `RedisError::SetHashFieldFailed` containing a description of the error if any failure occurs.
    ///
    /// # Examples
    /// ```
    /// let values = vec![("field1", "value1"), ("field2", "value2")];
    /// let result = your_redis_instance.set_hash_fields("your_hash", values, 3600).await;
    /// match result {
    ///     Ok(_) => println!("Hash fields set successfully!"),
    ///     Err(e) => println!("An error occurred: {:?}", e),
    /// }
    /// ```
    pub async fn set_hash_fields<V>(
        &self,
        key: &str,
        values: V,
        expiry: i64,
    ) -> Result<(), RedisError>
    where
        V: TryInto<RedisMap> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        self.pool
            .hset(key, values)
            .await
            .map_err(|err| RedisError::SetHashFieldFailed(err.to_string()))?;

        self.set_expiry(key, expiry).await?;
        Ok(())
    }

    /// Retrieves a field value from a hash in the Redis store.
    ///
    /// This asynchronous function receives a key representing a hash and a field within that hash,
    /// and attempts to retrieve the value associated with the field.
    /// It returns a `Result` containing the value of the specified type if the retrieval is successful,
    /// or an `RedisError::GetHashFieldFailed` containing a description of the error if any failure occurs.
    ///
    /// # Type Parameters
    /// - `V` - The type that the retrieved value will be converted into.
    ///   Must implement the `FromRedis` trait, and be `Unpin`, `Send`, and `'static`.
    ///
    /// # Parameters
    /// - `key: &str` - The key representing the hash in the Redis store.
    /// - `field: &str` - The field within the hash whose value should be retrieved.
    ///
    /// # Returns
    /// - `Result<V, RedisError>` - A `Result` containing the retrieved value of type `V` on success,
    ///   or an `RedisError::GetHashFieldFailed` on failure.
    ///
    /// # Examples
    /// ```
    /// let result = your_redis_instance.get_hash_field::<String>("your_hash", "your_field").await;
    /// match result {
    ///     Ok(value) => println!("Retrieved value: {:?}", value),
    ///     Err(e) => println!("An error occurred: {:?}", e),
    /// }
    /// ```
    pub async fn get_hash_field<V>(&self, key: &str, field: &str) -> Result<V, RedisError>
    where
        V: FromRedis + Unpin + Send + 'static,
    {
        self.pool
            .hget(key, field)
            .await
            .map_err(|err| RedisError::GetHashFieldFailed(err.to_string()))
    }

    /// Appends one or multiple values to the end of a list in the Redis store.
    ///
    /// This asynchronous function receives a key representing a list and a vector of values to be appended to the list.
    /// It attempts to append the values to the end of the list and returns the length of the list after the push operation.
    /// If the vector of values is empty, it will return the current length of the list without modifying it.
    ///
    /// # Type Parameters
    /// - `V` - The type of the values to be appended to the list. Must be convertible into a `RedisValue` and implements `Debug`, `Send`, `Sync`, and `Clone`.
    ///
    /// # Parameters
    /// - `key: &str` - The key representing the list in the Redis store.
    /// - `values: Vec<V>` - A vector of values to be appended to the end of the list.
    ///
    /// # Returns
    /// - `Result<i64, RedisError>` - A `Result` containing the length of the list after the push operation (`Ok`)
    ///   or an error (`Err`) with a description if any failure occurs.
    ///
    /// # Examples
    /// ```
    /// let values_to_push = vec!["value1", "value2"];
    /// let result = your_redis_instance.rpush("your_list", values_to_push).await;
    /// match result {
    ///     Ok(length) => println!("New length of the list: {}", length),
    ///     Err(e) => println!("An error occurred: {:?}", e),
    /// }
    /// ```
    pub async fn rpush<V>(&self, key: &str, values: Vec<V>) -> Result<i64, RedisError>
    where
        V: Serialize + Debug + Send + Sync + Clone,
    {
        if values.is_empty() {
            return self.llen(key).await;
        }

        let serialized_value = values
            .iter()
            .map(|value| {
                serde_json::to_string(value)
                    .map(Into::into)
                    .map_err(|err| RedisError::SerializationError(err.to_string()))
            })
            .collect::<Result<Vec<RedisValue>, RedisError>>()?;

        let output = self
            .pool
            .rpush(key, serialized_value)
            .await
            .map_err(|err| RedisError::RPushFailed(err.to_string()))?;

        match output {
            RedisValue::Integer(length) => Ok(length),
            case => Err(RedisError::RPushFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Appends one or multiple values to the end of a list in the Redis store and sets an expiry time for the key.
    ///
    /// This asynchronous function takes a key representing a list, a vector of values, and an expiry time in seconds.
    /// It atomically appends the values to the list and sets the expiry time for the key using a Redis pipeline.
    /// The function returns the length of the list after the push operation.
    ///
    /// # Type Parameters
    /// - `V` - The type of the values to be appended to the list. Must be convertible into a `RedisValue` and implements `Debug`, `Send`, `Sync`, and `Clone`.
    ///
    /// # Parameters
    /// - `key: &str` - The key representing the list in the Redis store.
    /// - `values: Vec<V>` - A vector of values to be appended to the end of the list.
    /// - `expiry: u32` - The expiry time in seconds to be set for the key.
    ///
    /// # Returns
    /// - `Result<i64, RedisError>` - A `Result` containing the length of the list after the push operation (`Ok`),
    ///   or an error (`Err`) with a description if any failure occurs.
    ///
    /// # Examples
    /// ```
    /// let values_to_push = vec!["value1", "value2"];
    /// let expiry_seconds = 300;
    /// let result = your_redis_instance.rpush_with_expiry("your_list", values_to_push, expiry_seconds).await;
    /// match result {
    ///     Ok(length) => println!("New length of the list: {}", length),
    ///     Err(e) => println!("An error occurred: {:?}", e),
    /// }
    /// ```
    pub async fn rpush_with_expiry<V>(
        &self,
        key: &str,
        values: Vec<V>,
        expiry: u32,
    ) -> Result<i64, RedisError>
    where
        V: Serialize + Debug + Send + Sync + Clone,
    {
        if values.is_empty() {
            return self.llen(key).await;
        }

        let pipeline = self.pool.pipeline();

        let serialized_value = values
            .iter()
            .map(|value| {
                serde_json::to_string(value)
                    .map(Into::into)
                    .map_err(|err| RedisError::SerializationError(err.to_string()))
            })
            .collect::<Result<Vec<RedisValue>, RedisError>>()?;

        let _ = pipeline
            .rpush::<RedisValue, &str, Vec<RedisValue>>(key, serialized_value)
            .await;
        let _ = pipeline.expire::<(), &str>(key, expiry.into()).await;

        let output: Vec<RedisValue> = pipeline
            .all()
            .await
            .map_err(|err| RedisError::RPushFailed(err.to_string()))?;

        match output.deref() {
            [RedisValue::Integer(length), ..] => Ok(length.to_owned()),
            case => Err(RedisError::RPushFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Pops one or multiple values from the end of a list in the Redis store.
    ///
    /// This asynchronous function removes and returns the last element(s) of the list stored at the specified key.
    /// The number of elements to be popped can be optionally specified. If no count is specified, one element is popped.
    /// It returns a vector of strings representing the popped values.
    ///
    /// # Parameters
    /// - `key: &str` - The key representing the list in the Redis store.
    /// - `count: Option<usize>` - An optional count specifying the number of elements to be popped from the end of the list.
    ///
    /// # Returns
    /// - `Result<Vec<String>, RedisError>` - A `Result` containing a vector of strings representing the popped values (`Ok`),
    ///   or an error (`Err`) with a description if any failure occurs.
    ///
    /// # Examples
    /// ```
    /// let result = your_redis_instance.rpop("your_list", Some(2)).await;
    /// match result {
    ///     Ok(values) => println!("Popped values: {:?}", values),
    ///     Err(e) => println!("An error occurred: {:?}", e),
    /// }
    /// ```
    pub async fn rpop<T>(&self, key: &str, count: Option<usize>) -> Result<Vec<T>, RedisError>
    where
        T: DeserializeOwned,
    {
        let output = self
            .pool
            .rpop(key, count)
            .await
            .map_err(|err| RedisError::RPopFailed(err.to_string()))?;

        match output {
            RedisValue::Array(val) => {
                let results = val
                    .into_iter()
                    .map(|v| match v {
                        RedisValue::String(s) => serde_json::from_str::<T>(&s)
                            .map_err(|err| RedisError::DeserializationError(err.to_string())),
                        case => Err(RedisError::RPopFailed(format!(
                            "Unexpected RedisValue encountered : {:?}",
                            case
                        ))),
                    })
                    .collect::<Result<Vec<T>, RedisError>>()?;
                Ok(results)
            }
            RedisValue::String(val) => serde_json::from_str::<T>(&val)
                .map(|val| vec![val])
                .map_err(|err| RedisError::DeserializationError(err.to_string())),
            case => Err(RedisError::RPopFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Removes and returns one or multiple elements from the start of a list in the Redis store.
    ///
    /// This asynchronous function pops the first element(s) of the list stored at the specified key.
    /// The count of elements to pop can be optionally provided. If no count is specified, a single element is popped.
    /// It returns a vector of strings representing the popped values if they exist.
    ///
    /// # Parameters
    /// - `key: &str` - The key associated with the list in Redis from which elements will be popped.
    /// - `count: Option<usize>` - An optional argument specifying the number of elements to pop from the start of the list.
    ///
    /// # Returns
    /// - `Result<Vec<String>, RedisError>` - A `Result` containing either:
    ///     - `Ok(Vec<String>)` - A vector of strings representing the popped values if successful.
    ///     - `Err(RedisError)` - An `RedisError` if the operation fails, containing a description of the error.
    ///
    /// # Examples
    /// ```rust
    /// async fn pop_elements(redis_instance: &YourRedisType, list_key: &str) {
    ///     let popped_elements = redis_instance.lpop(list_key, Some(3)).await;
    ///     match popped_elements {
    ///         Ok(values) => println!("Popped elements: {:?}", values),
    ///         Err(e) => println!("An error occurred while popping: {:?}", e),
    ///     }
    /// }
    /// ```
    ///
    /// Note: This function will return an empty vector if the list is empty or the key does not exist.
    pub async fn lpop<T>(&self, key: &str, count: Option<usize>) -> Result<Vec<T>, RedisError>
    where
        T: DeserializeOwned,
    {
        let output = self
            .pool
            .lpop(key, count)
            .await
            .map_err(|err| RedisError::LPopFailed(err.to_string()))?;

        match output {
            RedisValue::Array(val) => {
                let results = val
                    .into_iter()
                    .map(|v| match v {
                        RedisValue::String(s) => serde_json::from_str::<T>(&s)
                            .map_err(|err| RedisError::DeserializationError(err.to_string())),
                        case => Err(RedisError::LPopFailed(format!(
                            "Unexpected RedisValue encountered : {:?}",
                            case
                        ))),
                    })
                    .collect::<Result<Vec<T>, RedisError>>()?;
                Ok(results)
            }
            RedisValue::String(val) => serde_json::from_str::<T>(&val)
                .map(|val| vec![val])
                .map_err(|err| RedisError::DeserializationError(err.to_string())),
            RedisValue::Null => Ok(vec![]),
            case => Err(RedisError::LPopFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Retrieves a range of elements from a list in the Redis store.
    ///
    /// This asynchronous function returns a specified range of elements from the list stored at the provided key.
    /// The range is specified by the zero-based indexes `min` and `max`. If `max` is -1, the range will include all
    /// elements from `min` to the end of the list.
    ///
    /// # Parameters
    /// - `key: &str` - The key associated with the list in Redis from which elements will be retrieved.
    /// - `min: i64` - The zero-based index indicating the start of the range.
    /// - `max: i64` - The zero-based index indicating the end of the range. If set to -1, it will fetch till the end of the list.
    ///
    /// # Returns
    /// - `Result<Vec<String>, RedisError>` - A `Result` containing either:
    ///     - `Ok(Vec<String>)` - A vector of strings representing the list elements within the specified range.
    ///     - `Err(RedisError)` - An `RedisError` if the operation fails, containing a description of the error.
    ///
    /// # Examples
    /// ```rust
    /// async fn get_list_range(redis_instance: &YourRedisType, list_key: &str) {
    ///     let list_elements = redis_instance.lrange(list_key, 0, -1).await;
    ///     match list_elements {
    ///         Ok(elements) => println!("List elements within range: {:?}", elements),
    ///         Err(e) => println!("An error occurred while fetching the range: {:?}", e),
    ///     }
    /// }
    /// ```
    ///
    /// Note: This function will return an empty vector if the specified range does not contain any elements.
    pub async fn lrange(&self, key: &str, min: i64, max: i64) -> Result<Vec<String>, RedisError> {
        let output = self
            .pool
            .lrange(key, min, max)
            .await
            .map_err(|err| RedisError::LRangeFailed(err.to_string()))?;

        match output {
            RedisValue::Array(val) => {
                let mut values = Vec::new();
                for value in val {
                    if let RedisValue::String(y) = value {
                        values.push(String::from_utf8(y.into_inner().to_vec()).unwrap())
                    }
                }
                Ok(values)
            }
            case => Err(RedisError::LRangeFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Returns the length of the list stored at `key`.
    ///
    /// This asynchronous function gets the number of elements in the Redis list stored at the given `key`.
    ///
    /// # Parameters
    /// - `key: &str` - The key for the list whose length you want to retrieve.
    ///
    /// # Returns
    /// - `Result<i64, RedisError>` - A `Result` containing either:
    ///     - `Ok(i64)` - The length of the list as an `i64`.
    ///     - `Err(RedisError)` - An `RedisError` if the operation fails, with a description of the error.
    ///
    /// # Examples
    /// ```rust
    /// async fn get_list_length(redis_instance: &YourRedisType, list_key: &str) {
    ///     let length = redis_instance.llen(list_key).await;
    ///     match length {
    ///         Ok(len) => println!("Length of the list: {}", len),
    ///         Err(e) => println!("An error occurred: {}", e),
    ///     }
    /// }
    /// ```
    ///
    /// Note: This function will return 0 if the list does not exist.
    pub async fn llen(&self, key: &str) -> Result<i64, RedisError> {
        let output = self
            .pool
            .llen(key)
            .await
            .map_err(|err| RedisError::LLenFailed(err.to_string()))?;

        match output {
            RedisValue::Integer(length) => Ok(length),
            case => Err(RedisError::LLenFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Adds the specified geospatial items (longitude, latitude, name) to the specified key.
    ///
    /// # Arguments
    ///
    /// * `key` - A string slice that holds the name of the key to which geospatial items are added.
    /// * `values` - The geospatial items to add. This is a generic type that can be converted into
    ///              `MultipleGeoValues`, which represent multiple geospatial items.
    /// * `options` - Optional `SetOptions` to specify additional command options like `NX` or `XX`.
    /// * `changed` - A boolean indicating whether to return the number of elements that were
    ///               actually added to the set, not including all the elements already there.
    ///
    /// # Returns
    ///
    /// If successful, the function returns `Ok(())`, indicating that the geospatial items were added.
    /// If an error occurs, it returns an `Err(RedisError)` variant indicating the type of error.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn run() -> Result<(), RedisError> {
    /// # let redis_client = RedisClient::new(); // assuming a RedisClient struct that implements the method
    /// let key = "locations";
    /// let geospatial_data = vec![
    ///     GeoValue::new(13.361389, 38.115556, "Bangalore"),
    ///     GeoValue::new(15.087269, 37.502669, "Kolkata"),
    /// ];
    ///
    /// redis_client.geo_add(key, geospatial_data, None, false).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// This function will return an `Err` variant of `RedisError` with `GeoAddFailed` containing
    /// an error message if the Redis operation fails.
    pub async fn geo_add<V>(
        &self,
        key: &str,
        values: V,
        options: Option<SetOptions>,
        changed: bool,
    ) -> Result<(), RedisError>
    where
        V: Into<MultipleGeoValues> + Send + Debug,
    {
        self.pool
            .geoadd(key, options, changed, values)
            .await
            .map_err(|err| RedisError::GeoAddFailed(err.to_string()))
    }

    /// Adds geospatial items to the specified key with an expiry time.
    ///
    /// This function adds the specified geospatial items (longitude, latitude, name) to the specified
    /// key and sets an expiry time for the key.
    ///
    /// # Arguments
    ///
    /// * `key` - A string slice that holds the name of the key to which geospatial items are added.
    /// * `values` - The geospatial items to add. This is a generic type that can be converted into
    ///              `MultipleGeoValues`, which represent multiple geospatial items.
    /// * `options` - Optional `SetOptions` to specify additional command options like `NX` or `XX`.
    /// * `changed` - A boolean indicating whether to return the number of elements that were
    ///               actually added to the set, not including all the elements already there.
    /// * `expiry` - The expiry time in seconds after which the key will be deleted.
    ///
    /// # Returns
    ///
    /// If successful, the function returns `Ok(())`, indicating that the geospatial items were added
    /// to the key and the expiry was set. If an error occurs, it returns an `Err(RedisError)` variant
    /// indicating the type of error.
    pub async fn geo_add_with_expiry<V>(
        &self,
        key: &str,
        values: V,
        options: Option<SetOptions>,
        changed: bool,
        expiry: u64,
    ) -> Result<(), RedisError>
    where
        V: Into<MultipleGeoValues> + Send + Debug,
    {
        let pipeline = self.pool.pipeline();

        let _ = pipeline
            .geoadd::<RedisValue, &str, V>(key, options, changed, values)
            .await;
        let _ = pipeline.expire::<(), &str>(key, expiry as i64).await;

        pipeline
            .all()
            .await
            .map_err(|err| RedisError::GeoAddFailed(err.to_string()))
    }

    /// Adds multiple geospatial items with an expiry to various keys in a transactional way.
    ///
    /// For each key in the provided map, this function adds the specified geospatial items
    /// (longitude, latitude, name) and sets an expiry time for that key. The operations for all
    /// keys are batched in a Redis pipeline to ensure that they are executed atomically.
    ///
    /// # Arguments
    ///
    /// * `mval` - A reference to a `FxHashMap` where the key is a `String` representing the Redis key,
    ///            and the value is a `Vec<GeoValue>` representing geospatial items to be added.
    /// * `options` - Optional `SetOptions` to specify additional command options like `NX` or `XX`.
    /// * `changed` - A boolean indicating whether to return the number of elements that were
    ///               actually added to the set, not including all the elements already there.
    /// * `expiry` - The expiry time in seconds after which each key will be deleted.
    ///
    /// # Returns
    ///
    /// If successful, the function returns `Ok(())`, indicating that the geospatial items were added
    /// to their respective keys and the expiry was set for each. If an error occurs, it returns an
    /// `Err(RedisError)` variant indicating the type of error.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` variant of `RedisError` with `GeoAddFailed` containing
    /// an error message if the Redis operation fails for any of the keys.
    ///
    /// # Panics
    ///
    /// This function can panic if the underlying Redis driver encounters a critical error
    /// (e.g., connection loss). The use of a pipeline helps mitigate this by ensuring
    /// atomicity of the batch operation, but network issues can still lead to panics.
    /// Proper error handling is implemented to try to return an error variant instead
    /// of panicking.
    pub async fn mgeo_add_with_expiry(
        &self,
        mval: &FxHashMap<String, Vec<GeoValue>>,
        options: Option<SetOptions>,
        changed: bool,
        expiry: i64,
    ) -> Result<(), RedisError> {
        let pipeline = self.pool.pipeline();

        for (key, values) in mval.iter() {
            let _ = pipeline
                .geoadd::<RedisValue, &str, MultipleGeoValues>(
                    key,
                    options.to_owned(),
                    changed,
                    MultipleGeoValues::from(values.to_owned()),
                )
                .await;
            let _ = pipeline.expire::<(), &str>(key, expiry).await;
        }

        pipeline
            .all()
            .await
            .map_err(|err| RedisError::GeoAddFailed(err.to_string()))
    }

    /// Performs a search on a geospatial index to find items within a specified area.
    ///
    /// This function allows for various types of searches such as radius queries and bounding box queries.
    /// It can return additional information like the distance from the center point, coordinates, and
    /// the geohash of found items.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the geospatial index to search.
    /// * `from_member` - An optional `RedisValue` representing the member from which to start the search.
    /// * `from_lonlat` - An optional `GeoPosition` (longitude and latitude) representing the point from which
    ///                   to start the search.
    /// * `by_radius` - An optional tuple specifying the radius and unit (meters, kilometers, miles, feet) for radius searches.
    /// * `by_box` - An optional tuple specifying the width, height, and unit for bounding box searches.
    /// * `ord` - An optional `SortOrder` to sort the results by distance.
    /// * `count` - An optional tuple specifying the count and whether to return the exact or potential number of items.
    /// * `withcoord` - A boolean indicating whether to include coordinates in the results.
    /// * `withdist` - A boolean indicating whether to include distances in the results.
    /// * `withhash` - A boolean indicating whether to include geohashes in the results.
    ///
    /// # Returns
    ///
    /// If successful, the function returns `Ok(Vec<GeoRadiusInfo>)`, where `GeoRadiusInfo` contains information
    /// about each item found in the search. On failure, it returns an `Err(RedisError)` variant indicating the type of error.
    ///
    /// # Errors
    ///
    /// This function will return an `Err` variant of `RedisError` with `GeoSearchFailed` containing
    /// an error message if the Redis operation fails.
    ///
    /// # Panics
    ///
    /// This function should not panic under normal circumstances. However, unexpected issues with the
    /// Redis connection or internal errors from the Redis library may cause a panic. It is recommended
    /// to use a panic handler or similar safety net in production environments.
    #[allow(clippy::too_many_arguments)]
    pub async fn geo_search(
        &self,
        key: &str,
        from_lonlat: GeoPosition,
        by_radius: (f64, GeoUnit),
        ord: SortOrder,
    ) -> Result<Vec<GeoRadiusInfo>, RedisError> {
        self.pool
            .geosearch(
                key,
                None,
                Some(from_lonlat.to_owned()),
                Some(by_radius.to_owned()),
                None,
                Some(ord.to_owned()),
                None,
                true,
                false,
                false,
            )
            .await
            .map_err(|err| RedisError::GeoSearchFailed(err.to_string()))
    }

    /// Performs a geographical search on multiple Redis keys to find members within a specified area.
    ///
    /// # Arguments
    /// * `keys` - A vector of Redis key strings under which geo-spatial data is stored.
    /// * `from_member` - An optional Redis value specifying the name of a member around which to center the search.
    /// * `from_lonlat` - An optional `GeoPosition` specifying the longitude and latitude around which to center the search.
    /// * `by_radius` - An optional tuple specifying the radius and unit for the search area (e.g., (100.0, GeoUnit::Meters)).
    /// * `by_box` - An optional tuple specifying the width, height, and unit for the search area box.
    /// * `ord` - An optional `SortOrder` determining if the results should be sorted and how.
    /// * `count` - An optional tuple specifying the number of results to return and whether or not to consider it as "any" type.
    ///
    /// # Returns
    /// A `Result` wrapping a vector of `GeoRadiusInfo` which holds information about each found member,
    /// or an `RedisError` if an error occurs during the search.
    ///
    /// # Errors
    /// Returns `RedisError::GeoSearchFailed` if the Redis search fails or if an unexpected value is encountered.
    #[allow(clippy::too_many_arguments)]
    pub async fn mgeo_search(
        &self,
        keys: Vec<String>,
        from_lonlat: GeoPosition,
        by_radius: (f64, GeoUnit),
        ord: SortOrder,
    ) -> Result<Vec<Option<(String, Point)>>, RedisError> {
        let pipeline = self.pool.pipeline();

        for key in keys {
            let _ = pipeline
                .geosearch(
                    key,
                    None,
                    Some(from_lonlat.to_owned()),
                    Some(by_radius.to_owned()),
                    None,
                    Some(ord.to_owned()),
                    None,
                    true,
                    false,
                    false,
                )
                .await;
        }

        let geovals: Vec<Option<(String, Point)>> = pipeline
            .all::<Vec<Vec<RedisValue>>>()
            .await
            .map_err(|err| RedisError::GeoSearchFailed(err.to_string()))?
            .into_iter()
            .map(|geoval| {
                if let [RedisValue::String(member), RedisValue::Array(position)] = &geoval[..] {
                    if let [RedisValue::Double(longitude), RedisValue::Double(latitude)] =
                        position[..]
                    {
                        Some((
                            member.to_string(),
                            Point {
                                lon: longitude,
                                lat: latitude,
                            },
                        ))
                    } else {
                        error!("Unexpected RedisValue encountered");
                        None
                    }
                } else {
                    error!("Unexpected RedisValue encountered");
                    None
                }
            })
            .collect();

        Ok(geovals)
    }

    pub async fn geopos(&self, key: &str, members: Vec<String>) -> Result<Vec<Point>, RedisError> {
        let output = self
            .pool
            .geopos(key, members)
            .await
            .map_err(|err| RedisError::GeoPosFailed(err.to_string()))?;

        match output {
            RedisValue::Array(points) => {
                if !points.is_empty() {
                    if points[0].is_array() {
                        let mut resp = Vec::new();
                        for point in points {
                            let point = point.as_geo_position().unwrap();
                            if let Some(pos) = point {
                                resp.push(Point {
                                    lat: pos.latitude,
                                    lon: pos.longitude,
                                });
                            }
                        }
                        Ok(resp)
                    } else if points.len() == 2 && points[0].is_double() && points[1].is_double() {
                        return Ok(vec![Point {
                            lat: points[1].as_f64().unwrap(),
                            lon: points[0].as_f64().unwrap(),
                        }]);
                    } else {
                        return Ok(vec![]);
                    }
                } else {
                    Ok(vec![])
                }
            }
            case => Err(RedisError::GeoPosFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    /// Asynchronously removes all members in a sorted set within the specified ranks.
    ///
    /// This function interfaces with a Redis sorted set to remove members based on their rank in the set.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the Redis sorted set.
    /// * `start` - The starting rank (index) from which to remove members.
    /// * `stop` - The stopping rank (index) up to which members will be removed.
    ///
    /// # Returns
    ///
    /// * `()`: An empty tuple indicating successful completion.
    /// * `RedisError`: An error variant indicating a problem interfacing with Redis.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Omitted setup and initialization code
    ///
    /// let _ = zremrange_by_rank("sample_key", 0, 2).await?;
    /// ```
    pub async fn zremrange_by_rank(
        &self,
        key: &str,
        start: i64,
        stop: i64,
    ) -> Result<(), RedisError> {
        self.pool
            .zremrangebyrank(key, start, stop)
            .await
            .map_err(|err| RedisError::ZremrangeByRankFailed(err.to_string()))
    }

    /// Asynchronously adds one or multiple members to a sorted set, or updates its score if it already exists.
    ///
    /// This function interfaces with a Redis sorted set to add or update members.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the Redis sorted set.
    /// * `options` - An optional set of [`SetOptions`](https://docs.rs/redis/0.21.0/redis/enum.SetOptions.html) to specify additional behaviors.
    /// * `ordering` - Specifies the ordering for inserting the new values. Possible values are: None (the default), Some(Ordering::Greater), or Some(Ordering::Less).
    /// * `changed` - Indicates whether the ZADD operation should only add new elements and not update scores of elements that are already present.
    /// * `incr` - Indicates whether the operation should increment the score of an element if it's already present in the set.
    /// * `values` - A vector of tuples where each tuple contains a score and a member.
    ///
    /// # Returns
    ///
    /// * `()`: An empty tuple indicating successful completion.
    /// * `RedisError`: An error variant indicating a problem interfacing with Redis.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Omitted setup and initialization code
    ///
    /// let _ = zadd("sample_key", None, None, false, false, vec![(1.0, "member1"), (2.0, "member2")]).await?;
    /// ```
    pub async fn zadd(
        &self,
        key: &str,
        options: Option<SetOptions>,
        ordering: Option<Ordering>,
        changed: bool,
        incr: bool,
        values: Vec<(f64, &str)>,
    ) -> Result<(), RedisError> {
        self.pool
            .zadd(key, options, ordering, changed, incr, values)
            .await
            .map_err(|err| RedisError::ZAddFailed(err.to_string()))
    }

    /// Asynchronously retrieves the number of elements in a sorted set stored at the specified key.
    ///
    /// This function interfaces with a Redis sorted set to get the cardinality (number of members) of the set.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the Redis sorted set.
    ///
    /// # Returns
    ///
    /// * `u64`: The cardinality of the sorted set.
    /// * `RedisError`: An error variant indicating a problem interfacing with Redis.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Omitted setup and initialization code
    ///
    /// let count = zcard("sample_key").await?;
    /// println!("Number of members in sorted set: {}", count);
    /// ```
    pub async fn zcard(&self, key: &str) -> Result<u64, RedisError> {
        self.pool
            .zcard(key)
            .await
            .map_err(|err| RedisError::ZCardFailed(err.to_string()))
    }

    /// Asynchronously retrieves a range of elements from a sorted set stored at the specified key.
    ///
    /// This function interfaces with a Redis sorted set to get a range of elements, optionally with their scores,
    /// in a specified range. The range is defined by a minimum and maximum score.
    /// Additionally, it provides options to sort the results, reverse them, limit the number of results, and include the scores.
    ///
    /// # Arguments
    ///
    /// * `key` - The key of the Redis sorted set.
    /// * `min` - The minimum score for the range.
    /// * `max` - The maximum score for the range.
    /// * `sort` - Optional sort order for the results.
    /// * `rev` - Whether to reverse the result set.
    /// * `limit` - Optional limit to restrict the number of results.
    /// * `withscores` - Whether to include scores in the result.
    ///
    /// # Returns
    ///
    /// * `Vec<String>`: A vector containing the members from the sorted set that match the given criteria.
    /// * `RedisError`: An error variant indicating either a problem interfacing with Redis or unexpected data format.
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Omitted setup and initialization code
    ///
    /// let members = zrange("sample_key", 10, 20, None, false, Some(Limit(5)), false).await?;
    /// println!("{:?}", members);
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub async fn zrange<T>(
        &self,
        key: &str,
        min: i64,
        max: i64,
        sort: Option<ZSort>,
        rev: bool,
        limit: Option<Limit>,
        withscores: bool,
    ) -> Result<Vec<T>, RedisError>
    where
        T: DeserializeOwned,
    {
        let output = self
            .pool
            .zrange(key, min, max, sort, rev, limit, withscores)
            .await
            .map_err(|err| RedisError::ZRangeFailed(err.to_string()))?;

        match output {
            RedisValue::Array(val) => {
                let results = val
                    .into_iter()
                    .map(|v| match v {
                        RedisValue::String(s) => serde_json::from_str::<T>(&s)
                            .map_err(|err| RedisError::DeserializationError(err.to_string())),
                        case => Err(RedisError::ZRangeFailed(format!(
                            "Unexpected RedisValue encountered : {:?}",
                            case
                        ))),
                    })
                    .collect::<Result<Vec<T>, RedisError>>()?;
                Ok(results)
            }
            RedisValue::String(val) => serde_json::from_str::<T>(&val)
                .map(|val| vec![val])
                .map_err(|err| RedisError::DeserializationError(err.to_string())),
            case => Err(RedisError::ZRangeFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    pub async fn xadd<F, V>(
        &self,
        key: &str,
        fields: Vec<(F, V)>,
        trim_threshold: i64,
    ) -> Result<(), RedisError>
    where
        F: Into<RedisKey> + Send,
        V: Into<RedisValue> + Send,
    {
        self.pool
            .xadd(
                key,
                false,
                (
                    XCapKind::MaxLen,
                    XCapTrim::AlmostExact,
                    StringOrNumber::Number(trim_threshold),
                    None,
                ),
                Auto,
                fields,
            )
            .await
            .map_err(|err| RedisError::XAddFailed(err.to_string()))?;

        Ok(())
    }

    pub async fn xread(
        &self,
        keys: Vec<String>,
        ids: Vec<String>,
    ) -> Result<FxHashMap<String, Vec<Vec<(String, String)>>>, RedisError> {
        let output: RedisValue = self
            .pool
            .xread(
                None,
                None,
                keys,
                ids.iter().map(|id| Manual(id.into())).collect::<Vec<XID>>(),
            )
            .await
            .map_err(|err| RedisError::XReadFailed(err.to_string()))?;

        let mut result = FxHashMap::default();

        match output {
            RedisValue::Map(output) => {
                for (redis_key, value_array) in output.inner() {
                    if let RedisValue::Array(value_array) = value_array {
                        // Convert RedisKey to String key
                        let key = redis_key.into_string().unwrap();

                        let mut entries = Vec::new();

                        for value in value_array {
                            if let RedisValue::Array(entry_array) = value {
                                // Assuming the first element is a stream ID and the second element is an array of field-value pairs
                                let mut field_values = Vec::new();

                                // Extract the stream ID, assuming it's the first element in the array.
                                if let Some(RedisValue::String(id)) = entry_array.get(0) {
                                    field_values.push(("id".to_string(), id.to_string()));
                                }

                                // Extract the field-value pairs, assuming they start from the second element.
                                if let Some(RedisValue::Array(fields)) = entry_array.get(1) {
                                    for field in fields.chunks(2) {
                                        if let [RedisValue::String(field_name), RedisValue::String(field_value)] =
                                            field
                                        {
                                            field_values.push((
                                                field_name.to_string(),
                                                field_value.to_string(),
                                            ));
                                        }
                                    }
                                }

                                entries.push(field_values);
                            }
                        }

                        result.insert(key, entries);
                    }
                }

                Ok(result)
            }
            case => Err(RedisError::XReadFailed(format!(
                "Unexpected RedisValue encountered : {:?}",
                case
            ))),
        }
    }

    pub async fn xdel(&self, key: &str, id: &str) -> Result<(), RedisError> {
        self.pool
            .xdel(key, id)
            .await
            .map_err(|err| RedisError::XDeleteFailed(err.to_string()))
    }
}
