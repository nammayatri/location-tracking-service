/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::unwrap_used)]

use crate::redis::types::*;
use crate::tools::error::AppError;
use fred::{
    interfaces::{GeoInterface, HashesInterface, KeysInterface, SortedSetsInterface},
    prelude::{ListInterface, RedisError},
    types::{
        Expiration, FromRedis, GeoPosition, GeoRadiusInfo, GeoUnit, GeoValue, Limit,
        MultipleGeoValues, MultipleKeys, Ordering, RedisKey, RedisMap, RedisValue, SetOptions,
        SortOrder, ZSort,
    },
};
use rustc_hash::FxHashMap;
use std::fmt::Debug;

impl RedisConnectionPool {
    // set key
    pub async fn set_key<V>(&self, key: &str, value: V, expiry: u32) -> Result<(), AppError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = if self.migration_pool.is_some() {
            self.migration_pool
                .as_ref()
                .unwrap()
                .set(key, value, Some(Expiration::EX(expiry.into())), None, false)
                .await
        } else {
            self.pool
                .set(key, value, Some(Expiration::EX(expiry.into())), None, false)
                .await
        };

        if let Err(err) = output {
            Err(AppError::SetFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    // setnx key with expiry
    pub async fn setnx_with_expiry<V>(
        &self,
        key: &str,
        value: V,
        expiry: i64,
    ) -> Result<(), AppError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let pipeline = self.pool.pipeline();

        let _ = pipeline.msetnx::<RedisValue, _>((key, value)).await;
        let _ = pipeline.expire::<(), &str>(key, expiry).await;

        let output: Result<Vec<RedisValue>, RedisError> = pipeline.all().await;

        match output.as_deref() {
            Ok([RedisValue::Integer(1), ..]) => Ok(()),
            Err(err) => Err(AppError::SetExFailed(err.to_string())),
            Ok(case) => Err(AppError::SetExFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    pub async fn set_expiry(&self, key: &str, seconds: i64) -> Result<(), AppError> {
        let output: Result<(), _> = self.pool.expire(key, seconds).await;

        if let Err(err) = output {
            Err(AppError::SetExpiryFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    // get key
    pub async fn get_key(&self, key: &str) -> Result<Option<String>, AppError> {
        let output: Result<RedisValue, _> = if self.migration_pool.is_some() {
            let val = self.migration_pool.as_ref().unwrap().get(key).await;
            if let Ok(RedisValue::Null) = val {
                self.pool.get(key).await
            } else {
                val
            }
        } else {
            self.pool.get(key).await
        };

        match output {
            Ok(RedisValue::String(val)) => Ok(Some(val.to_string())),
            Ok(RedisValue::Null) => Ok(None),
            Err(err) => Err(AppError::GetFailed(err.to_string())),
            Ok(case) => Err(AppError::GetFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    // mget key
    pub async fn mget_keys(&self, keys: Vec<String>) -> Result<Vec<Option<String>>, AppError> {
        if keys.is_empty() {
            return Ok(vec![None]);
        }

        let keys: Vec<RedisKey> = keys.into_iter().map(RedisKey::from).collect();

        let output: Result<RedisValue, _> = if self.migration_pool.is_some() {
            let val = self
                .migration_pool
                .as_ref()
                .unwrap()
                .mget(MultipleKeys::from(keys.clone()))
                .await;
            if let Ok(RedisValue::Null) = val {
                self.pool.mget(MultipleKeys::from(keys)).await
            } else {
                val
            }
        } else {
            self.pool.mget(MultipleKeys::from(keys)).await
        };

        match output {
            Ok(RedisValue::Array(val)) => Ok(val.into_iter().map(|v| v.into_string()).collect()),
            Ok(RedisValue::String(val)) => Ok(vec![Some(val.to_string())]),
            Ok(RedisValue::Null) => Ok(vec![None]),
            Err(err) => Err(AppError::MGetFailed(err.to_string())),
            Ok(case) => Err(AppError::MGetFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    // delete key
    pub async fn delete_key(&self, key: &str) -> Result<(), AppError> {
        let output: Result<(), _> = self.pool.del(key).await;

        if let Err(err) = output {
            Err(AppError::DeleteFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    // delete multiple keys
    pub async fn delete_keys(&self, keys: Vec<&str>) -> Result<(), AppError> {
        let pipeline = if self.migration_pool.is_some() {
            self.migration_pool.as_ref().unwrap().pipeline()
        } else {
            self.pool.pipeline()
        };

        for key in keys {
            let _ = pipeline.del::<RedisValue, &str>(key).await;
        }

        let output: Result<Vec<RedisValue>, RedisError> = pipeline.all().await;

        if let Err(err) = output {
            Err(AppError::DeleteFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    //HSET
    pub async fn set_hash_fields<V>(
        &self,
        key: &str,
        values: V,
        expiry: i64,
    ) -> Result<(), AppError>
    where
        V: TryInto<RedisMap> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self.pool.hset(key, values).await;

        // setting expiry for the key
        if let Err(err) = output {
            Err(AppError::SetHashFieldFailed(err.to_string()))
        } else {
            self.set_expiry(key, expiry).await?;
            Ok(())
        }
    }

    //HGET
    pub async fn get_hash_field<V>(&self, key: &str, field: &str) -> Result<V, AppError>
    where
        V: FromRedis + Unpin + Send + 'static,
    {
        let output: Result<V, _> = self.pool.hget(key, field).await;

        if output.is_err() {
            return Err(AppError::GetHashFieldFailed("Case not handled".to_string()));
        }

        Ok(output.unwrap())
    }

    //RPUSH
    pub async fn rpush<V>(&self, key: &str, values: Vec<V>) -> Result<i64, AppError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync + Clone,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        if values.is_empty() {
            return self.llen(key).await;
        }

        let output = self.pool.rpush(key, values).await;

        match output {
            Ok(RedisValue::Integer(length)) => Ok(length),
            Err(err) => Err(AppError::RPushFailed(err.to_string())),
            Ok(case) => Err(AppError::RPushFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    //RPUSH with expiry
    pub async fn rpush_with_expiry<V>(
        &self,
        key: &str,
        values: Vec<V>,
        expiry: u32,
    ) -> Result<i64, AppError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync + Clone,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        if values.is_empty() {
            return self.llen(key).await;
        }

        let output: Result<Vec<RedisValue>, RedisError> = if self.migration_pool.is_some() {
            let x: Result<i64, RedisError> = self.pool.exists(vec![key]).await;
            // info!("key {:?} exists xyz {:?}", key, x);
            if x.unwrap() == 1 {
                let pipeline = self.pool.pipeline();
                let _ = pipeline
                    .rpush::<RedisValue, &str, Vec<V>>(key, values)
                    .await;
                let _ = pipeline.expire::<(), &str>(key, expiry.into()).await;
                pipeline.all().await
            } else {
                let migration_pipeline = self.migration_pool.as_ref().unwrap().pipeline();
                let _ = migration_pipeline
                    .rpush::<RedisValue, &str, Vec<V>>(key, values)
                    .await;
                let _ = migration_pipeline
                    .expire::<(), &str>(key, expiry.into())
                    .await;
                migration_pipeline.all().await
            }
        } else {
            let pipeline = self.pool.pipeline();
            let _ = pipeline
                .rpush::<RedisValue, &str, Vec<V>>(key, values)
                .await;
            let _ = pipeline.expire::<(), &str>(key, expiry.into()).await;
            pipeline.all().await
        };

        match output.as_deref() {
            Ok([RedisValue::Integer(length), ..]) => Ok(length.to_owned()),
            Err(err) => Err(AppError::RPushFailed(err.to_string())),
            Ok(case) => Err(AppError::RPushFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    //RPOP
    pub async fn rpop(&self, key: &str, count: Option<usize>) -> Result<Vec<String>, AppError> {
        let output = self.pool.rpop(key, count).await;

        match output {
            Ok(RedisValue::Array(val)) => {
                let mut values = Vec::new();
                for value in val {
                    if let RedisValue::String(y) = value {
                        values.push(String::from_utf8(y.into_inner().to_vec()).unwrap())
                    }
                }
                Ok(values)
            }
            Ok(RedisValue::String(value)) => Ok(vec![value.to_string()]),
            Err(err) => Err(AppError::RPopFailed(err.to_string())),
            Ok(case) => Err(AppError::RPopFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    //LPOP
    pub async fn lpop(&self, key: &str, count: Option<usize>) -> Result<Vec<String>, AppError> {
        let output = if self.migration_pool.is_some() {
            let x: Result<i64, RedisError> = self.pool.exists(vec![key]).await;
            // info!("key {:?} exists xyz {:?}", key, x);
            if x.unwrap() == 1 {
                self.pool.lpop(key, count).await
            } else {
                self.migration_pool.as_ref().unwrap().lpop(key, count).await
            }
        } else {
            self.pool.lpop(key, count).await
        };

        match output {
            Ok(RedisValue::Array(val)) => {
                let mut values = Vec::new();
                for value in val {
                    if let RedisValue::String(y) = value {
                        values.push(String::from_utf8(y.into_inner().to_vec()).unwrap())
                    }
                }
                Ok(values)
            }
            Ok(RedisValue::String(value)) => Ok(vec![value.to_string()]),
            Ok(RedisValue::Null) => Ok(vec![]),
            Err(err) => Err(AppError::LPopFailed(err.to_string())),
            Ok(case) => Err(AppError::LPopFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    //LRANGE
    pub async fn lrange(&self, key: &str, min: i64, max: i64) -> Result<Vec<String>, AppError> {
        let output = self.pool.lrange(key, min, max).await;

        match output {
            Ok(RedisValue::Array(val)) => {
                let mut values = Vec::new();
                for value in val {
                    if let RedisValue::String(y) = value {
                        values.push(String::from_utf8(y.into_inner().to_vec()).unwrap())
                    }
                }
                Ok(values)
            }
            Err(err) => Err(AppError::LRangeFailed(err.to_string())),
            Ok(case) => Err(AppError::LRangeFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    //LLEN
    pub async fn llen(&self, key: &str) -> Result<i64, AppError> {
        let output = if self.migration_pool.is_some() {
            let x: Result<i64, RedisError> = self.pool.exists(vec![key]).await;
            if x.unwrap() == 1 {
                self.pool.llen(key).await
            } else {
                self.migration_pool.as_ref().unwrap().llen(key).await
            }
        } else {
            self.pool.llen(key).await
        };

        match output {
            Ok(RedisValue::Integer(length)) => Ok(length),
            Err(err) => Err(AppError::LLenFailed(err.to_string())),
            Ok(case) => Err(AppError::LLenFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    //GEOADD
    pub async fn geo_add<V>(
        &self,
        key: &str,
        values: V,
        options: Option<SetOptions>,
        changed: bool,
    ) -> Result<(), AppError>
    where
        V: Into<MultipleGeoValues> + Send + Debug,
    {
        let output: Result<(), _> = self.pool.geoadd(key, options, changed, values).await;

        if let Err(err) = output {
            Err(AppError::GeoAddFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    //GEOADD with expiry
    pub async fn geo_add_with_expiry<V>(
        &self,
        key: &str,
        values: V,
        options: Option<SetOptions>,
        changed: bool,
        expiry: u64,
    ) -> Result<(), AppError>
    where
        V: Into<MultipleGeoValues> + Send + Debug,
    {
        let pipeline = self.pool.pipeline();

        let _ = pipeline
            .geoadd::<RedisValue, &str, V>(key, options, changed, values)
            .await;
        let _ = pipeline.expire::<(), &str>(key, expiry as i64).await;

        let output: Result<Vec<RedisValue>, RedisError> = pipeline.all().await;

        if let Err(err) = output {
            Err(AppError::GeoAddFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    //MGEOADD with expiry
    pub async fn mgeo_add_with_expiry(
        &self,
        mval: &FxHashMap<String, Vec<GeoValue>>,
        options: Option<SetOptions>,
        changed: bool,
        expiry: i64,
    ) -> Result<(), AppError> {
        let output: Result<Vec<RedisValue>, RedisError> = if self.migration_pool.is_some() {
            let pipeline = self.pool.pipeline();
            let migration_pipeline = self.migration_pool.as_ref().unwrap().pipeline();
            for (key, values) in mval.iter() {
                let x: Result<i64, RedisError> = self.pool.exists(vec![key]).await;
                if x.unwrap() == 1 {
                    let _ = pipeline
                        .geoadd::<RedisValue, &str, MultipleGeoValues>(
                            key,
                            options.to_owned(),
                            changed,
                            MultipleGeoValues::from(values.to_owned()),
                        )
                        .await;
                    let _ = pipeline.expire::<(), &str>(key, expiry).await;
                } else {
                    let _ = migration_pipeline
                        .geoadd::<RedisValue, &str, MultipleGeoValues>(
                            key,
                            options.to_owned(),
                            changed,
                            MultipleGeoValues::from(values.to_owned()),
                        )
                        .await;
                    let _ = migration_pipeline.expire::<(), &str>(key, expiry).await;
                }
            }
            let output: Result<Vec<RedisValue>, RedisError> = pipeline.all().await;
            let migration_output: Result<Vec<RedisValue>, RedisError> =
                migration_pipeline.all().await;

            if let Err(err) = output {
                Err(err)
            } else if let Err(err) = migration_output {
                Err(err)
            } else if let Ok(migration_output) = migration_output {
                if let Ok(mut output) = output {
                    output.extend(migration_output);
                    Ok(output)
                } else {
                    Ok(migration_output)
                }
            } else {
                output
            }
        } else {
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
            pipeline.all().await
        };

        if let Err(err) = output {
            Err(AppError::GeoAddFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    //GEOSEARCH
    #[allow(clippy::too_many_arguments)]
    pub async fn geo_search(
        &self,
        key: &str,
        from_member: Option<RedisValue>,
        from_lonlat: Option<GeoPosition>,
        by_radius: Option<(f64, GeoUnit)>,
        by_box: Option<(f64, f64, GeoUnit)>,
        ord: Option<SortOrder>,
        count: Option<(u64, fred::types::Any)>,
        withcoord: bool,
        withdist: bool,
        withhash: bool,
    ) -> Result<Vec<GeoRadiusInfo>, AppError> {
        let output: Result<Vec<GeoRadiusInfo>, _> = self
            .pool
            .geosearch(
                key,
                from_member,
                from_lonlat,
                by_radius,
                by_box,
                ord,
                count,
                withcoord,
                withdist,
                withhash,
            )
            .await;

        match output {
            Ok(output) => Ok(output),
            Err(err) => Err(AppError::GeoSearchFailed(err.to_string())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mgeo_search(
        &self,
        keys: Vec<String>,
        from_member: Option<RedisValue>,
        from_lonlat: Option<GeoPosition>,
        by_radius: Option<(f64, GeoUnit)>,
        by_box: Option<(f64, f64, GeoUnit)>,
        ord: Option<SortOrder>,
        count: Option<(u64, fred::types::Any)>,
    ) -> Result<Vec<GeoRadiusInfo>, AppError> {
        let output: Result<Vec<Vec<RedisValue>>, RedisError> = if self.migration_pool.is_some() {
            let pipeline = self.pool.pipeline();
            let migration_pipeline = self.migration_pool.as_ref().unwrap().pipeline();

            for key in keys.clone() {
                // let x: Result<i64, _>  = self.pool.exists(vec![key.clone()]).await;

                let _ = pipeline
                    .geosearch(
                        key.clone(),
                        from_member.to_owned(),
                        from_lonlat.to_owned(),
                        by_radius.to_owned(),
                        by_box.to_owned(),
                        ord.to_owned(),
                        count,
                        true,
                        false,
                        false,
                    )
                    .await;
                let _ = migration_pipeline
                    .geosearch(
                        key,
                        from_member.to_owned(),
                        from_lonlat.to_owned(),
                        by_radius.to_owned(),
                        by_box.to_owned(),
                        ord.to_owned(),
                        count,
                        true,
                        false,
                        false,
                    )
                    .await;
            }

            let output: Result<Vec<Vec<RedisValue>>, RedisError> = pipeline.all().await;
            let migration_output: Result<Vec<Vec<RedisValue>>, RedisError> =
                migration_pipeline.all().await;

            if let Ok(migration_output) = migration_output {
                if let Ok(mut output) = output {
                    output.extend(migration_output);
                    Ok(output)
                } else {
                    Ok(migration_output)
                }
            } else {
                output
            }
        } else {
            let pipeline = self.pool.pipeline();
            for key in keys {
                let _ = pipeline
                    .geosearch(
                        key,
                        from_member.to_owned(),
                        from_lonlat.to_owned(),
                        by_radius.to_owned(),
                        by_box.to_owned(),
                        ord.to_owned(),
                        count,
                        true,
                        false,
                        false,
                    )
                    .await;
            }
            let output: Result<Vec<Vec<RedisValue>>, RedisError> = pipeline.all().await;
            output
        };

        match output {
            Ok(geovals) => {
                let mut output = Vec::new();

                for geoval in geovals {
                    if let [member, RedisValue::Array(position)] = &geoval[..] {
                        if let [RedisValue::Double(longitude), RedisValue::Double(latitude)] =
                            position[..]
                        {
                            output.push(GeoRadiusInfo {
                                member: member.to_owned(),
                                position: Some(GeoPosition {
                                    longitude,
                                    latitude,
                                }),
                                distance: None,
                                hash: None,
                            })
                        }
                    }
                }
                Ok(output)
            }
            Err(err) => Err(AppError::GeoSearchFailed(err.to_string())),
        }
    }

    //GEOPOS
    pub async fn geopos(&self, key: &str, members: Vec<String>) -> Result<Vec<Point>, AppError> {
        let output: Result<RedisValue, _> = self.pool.geopos(key, members).await;

        match output {
            Ok(RedisValue::Array(points)) => {
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
            Err(err) => Err(AppError::GeoPosFailed(err.to_string())),
            Ok(case) => Err(AppError::GeoPosFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }

    //ZREMRANGEBYRANK
    pub async fn zremrange_by_rank(
        &self,
        key: &str,
        start: i64,
        stop: i64,
    ) -> Result<(), AppError> {
        let output: Result<(), _> = self.pool.zremrangebyrank(key, start, stop).await;

        if let Err(err) = output {
            Err(AppError::ZremrangeByRankFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    //ZADD
    pub async fn zadd(
        &self,
        key: &str,
        options: Option<SetOptions>,
        ordering: Option<Ordering>,
        changed: bool,
        incr: bool,
        values: Vec<(f64, &str)>,
    ) -> Result<(), AppError> {
        let output: Result<(), _> = self
            .pool
            .zadd(key, options, ordering, changed, incr, values)
            .await;

        if let Err(err) = output {
            Err(AppError::ZAddFailed(err.to_string()))
        } else {
            Ok(())
        }
    }

    //ZCARD
    pub async fn zcard(&self, key: &str) -> Result<u64, AppError> {
        let output: Result<u64, _> = self.pool.zcard(key).await;

        if output.is_err() {
            return Err(AppError::ZCardFailed("Case not handled".to_string()));
        }

        Ok(output.unwrap())
    }

    //ZRANGE
    #[allow(clippy::too_many_arguments)]
    pub async fn zrange(
        &self,
        key: &str,
        min: i64,
        max: i64,
        sort: Option<ZSort>,
        rev: bool,
        limit: Option<Limit>,
        withscores: bool,
    ) -> Result<Vec<String>, AppError> {
        let output: Result<RedisValue, _> = self
            .pool
            .zrange(key, min, max, sort, rev, limit, withscores)
            .await;

        match output {
            Ok(RedisValue::Array(val)) => {
                let mut members = Vec::new();
                for member in val {
                    if let RedisValue::String(y) = member {
                        members.push(String::from_utf8(y.into_inner().to_vec()).unwrap())
                    }
                }
                members.sort();
                Ok(members)
            }
            Ok(RedisValue::String(member)) => Ok(vec![member.to_string()]),
            Err(err) => Err(AppError::ZRangeFailed(err.to_string())),
            Ok(case) => Err(AppError::ZRangeFailed(format!(
                "Case not handled : {:?}",
                case
            ))),
        }
    }
}
