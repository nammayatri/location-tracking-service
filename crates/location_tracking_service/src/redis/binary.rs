/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

//! Binary (MessagePack) serialization for Redis values.
//!
//! Provides ~30-50% smaller payloads and faster ser/de compared to JSON.
//! Falls back to JSON deserialization for backward compatibility with
//! existing data written before the binary migration.

use crate::tools::error::AppError;
use fred::prelude::KeysInterface;
use fred::types::{Expiration, RedisValue};
use serde::{de::DeserializeOwned, Serialize};
use shared::redis::types::RedisConnectionPool;
use std::ops::Deref;

/// Stores a value in Redis using MessagePack binary serialization.
pub async fn set_key_binary<V>(
    redis: &RedisConnectionPool,
    key: &str,
    value: &V,
    expiry: u32,
) -> Result<(), AppError>
where
    V: Serialize,
{
    let bytes = rmp_serde::to_vec(value)
        .map_err(|err| AppError::InternalError(format!("msgpack serialize: {err}")))?;

    redis
        .writer_pool
        .set::<(), _, _>(
            key,
            RedisValue::Bytes(bytes.into()),
            Some(Expiration::EX(expiry.into())),
            None,
            false,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Stores a value in Redis using MessagePack without expiry.
pub async fn set_key_binary_no_expiry<V>(
    redis: &RedisConnectionPool,
    key: &str,
    value: &V,
) -> Result<(), AppError>
where
    V: Serialize,
{
    let bytes = rmp_serde::to_vec(value)
        .map_err(|err| AppError::InternalError(format!("msgpack serialize: {err}")))?;

    redis
        .writer_pool
        .set::<(), _, _>(key, RedisValue::Bytes(bytes.into()), None, None, false)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Retrieves a value from Redis, trying MessagePack first then falling back to JSON.
///
/// This dual-decode approach enables zero-downtime migration: new writes use
/// binary format while old JSON values are still readable until they expire.
pub async fn get_key_binary<T>(
    redis: &RedisConnectionPool,
    key: &str,
) -> Result<Option<T>, AppError>
where
    T: DeserializeOwned,
{
    let output: RedisValue = redis
        .reader_pool
        .get(key)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    match output {
        RedisValue::Bytes(bytes) => rmp_serde::from_slice(&bytes)
            .map(Some)
            .map_err(|err| AppError::InternalError(format!("msgpack deserialize: {err}"))),
        RedisValue::String(val) => {
            // Backward compatibility: try JSON for pre-migration data
            serde_json::from_str(val.deref())
                .map(Some)
                .map_err(|err| AppError::InternalError(format!("json fallback deserialize: {err}")))
        }
        RedisValue::Null => Ok(None),
        other => Err(AppError::InternalError(format!(
            "unexpected RedisValue: {other:?}"
        ))),
    }
}

/// Batch-retrieve multiple keys with MessagePack/JSON dual decoding.
pub async fn mget_keys_binary<T>(
    redis: &RedisConnectionPool,
    keys: Vec<String>,
) -> Result<Vec<Option<T>>, AppError>
where
    T: DeserializeOwned,
{
    if keys.is_empty() {
        return Ok(vec![]);
    }

    let redis_keys: Vec<fred::types::RedisKey> =
        keys.into_iter().map(fred::types::RedisKey::from).collect();

    let output: RedisValue = redis
        .reader_pool
        .mget(fred::types::MultipleKeys::from(redis_keys))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    match output {
        RedisValue::Array(values) => values
            .into_iter()
            .map(|v| match v {
                RedisValue::Bytes(bytes) => rmp_serde::from_slice(&bytes)
                    .map(Some)
                    .map_err(|err| AppError::InternalError(format!("msgpack deserialize: {err}"))),
                RedisValue::String(s) => serde_json::from_str(s.deref()).map(Some).map_err(|err| {
                    AppError::InternalError(format!("json fallback deserialize: {err}"))
                }),
                RedisValue::Null => Ok(None),
                other => Err(AppError::InternalError(format!(
                    "unexpected RedisValue: {other:?}"
                ))),
            })
            .collect(),
        RedisValue::Null => Ok(vec![None]),
        other => Err(AppError::InternalError(format!(
            "unexpected mget result: {other:?}"
        ))),
    }
}

/// Binary RPUSH with expiry using a pipeline.
pub async fn rpush_binary_with_expiry<V>(
    redis: &RedisConnectionPool,
    key: &str,
    values: Vec<V>,
    expiry: u32,
) -> Result<i64, AppError>
where
    V: Serialize,
{
    use fred::prelude::ListInterface;

    if values.is_empty() {
        let len: RedisValue = redis
            .reader_pool
            .llen(key)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        return match len {
            RedisValue::Integer(n) => Ok(n),
            _ => Ok(0),
        };
    }

    let serialized: Vec<RedisValue> = values
        .iter()
        .map(|v| {
            rmp_serde::to_vec(v)
                .map(|b| RedisValue::Bytes(b.into()))
                .map_err(|err| AppError::InternalError(format!("msgpack serialize: {err}")))
        })
        .collect::<Result<_, _>>()?;

    let pipeline = redis.writer_pool.next().pipeline();
    let _ = pipeline
        .rpush::<RedisValue, _, Vec<RedisValue>>(key, serialized)
        .await;
    let _ = pipeline.expire::<(), _>(key, expiry as i64).await;

    let output: Vec<RedisValue> = pipeline
        .all()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    match output.deref() {
        [RedisValue::Integer(length), ..] => Ok(*length),
        other => Err(AppError::InternalError(format!(
            "unexpected rpush result: {other:?}"
        ))),
    }
}

/// Binary LPOP with MessagePack/JSON dual decoding.
pub async fn lpop_binary<T>(
    redis: &RedisConnectionPool,
    key: &str,
    count: Option<usize>,
) -> Result<Vec<T>, AppError>
where
    T: DeserializeOwned,
{
    use fred::prelude::ListInterface;

    let output: RedisValue = redis
        .writer_pool
        .lpop(key, count)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    decode_list_result(output)
}

/// Binary LRANGE with MessagePack/JSON dual decoding.
pub async fn lrange_binary<T>(
    redis: &RedisConnectionPool,
    key: &str,
    start: i64,
    stop: i64,
) -> Result<Vec<T>, AppError>
where
    T: DeserializeOwned,
{
    use fred::prelude::ListInterface;

    let output: RedisValue = redis
        .reader_pool
        .lrange(key, start, stop)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    decode_list_result(output)
}

fn decode_list_result<T: DeserializeOwned>(output: RedisValue) -> Result<Vec<T>, AppError> {
    match output {
        RedisValue::Array(vals) => vals
            .into_iter()
            .map(|v| match v {
                RedisValue::Bytes(bytes) => rmp_serde::from_slice(&bytes)
                    .map_err(|err| AppError::InternalError(format!("msgpack deserialize: {err}"))),
                RedisValue::String(s) => serde_json::from_str(s.deref())
                    .map_err(|err| AppError::InternalError(format!("json fallback: {err}"))),
                other => Err(AppError::InternalError(format!(
                    "unexpected list element: {other:?}"
                ))),
            })
            .collect(),
        RedisValue::Bytes(bytes) => rmp_serde::from_slice(&bytes)
            .map(|v| vec![v])
            .map_err(|err| AppError::InternalError(format!("msgpack deserialize: {err}"))),
        RedisValue::String(s) => serde_json::from_str(s.deref())
            .map(|v| vec![v])
            .map_err(|err| AppError::InternalError(format!("json fallback: {err}"))),
        RedisValue::Null => Ok(vec![]),
        other => Err(AppError::InternalError(format!(
            "unexpected list result: {other:?}"
        ))),
    }
}
