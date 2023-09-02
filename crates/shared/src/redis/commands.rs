use crate::redis::types::*;
use crate::tools::error::AppError;
use crate::utils::logger::instrument;
use error_stack::{IntoReport, ResultExt};
use fred::{
    interfaces::{GeoInterface, HashesInterface, KeysInterface, SortedSetsInterface},
    prelude::ListInterface,
    types::{
        Expiration, FromRedis, GeoPosition, GeoRadiusInfo, GeoUnit, Limit, MultipleGeoValues,
        Ordering, RedisMap, RedisValue, SetOptions, SortOrder, ZSort,
    },
};
use std::fmt::Debug;

impl RedisConnectionPool {
    // set key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_key<V>(&self, key: &str, value: V) -> Result<(), AppError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .set(
                key,
                value,
                Some(Expiration::EX(self.config.default_ttl.into())),
                None,
                false,
            )
            .await
            .into_report()
            .change_context(AppError::SetFailed);

        if output.is_err() {
            return Err(AppError::SetFailed);
        }

        Ok(())
    }

    // set key with expiry
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_with_expiry<V>(&self, key: &str, value: V, expiry: u32) -> Result<(), AppError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .set(key, value, Some(Expiration::EX(expiry.into())), None, false)
            .await
            .into_report()
            .change_context(AppError::SetFailed);

        if output.is_err() {
            return Err(AppError::SetFailed);
        }

        Ok(())
    }

    // setnx key with expiry
    #[instrument(level = "DEBUG", skip(self))]
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
        let output: Result<RedisValue, _> = self
            .pool
            .msetnx((key, value))
            .await
            .into_report()
            .change_context(AppError::SetFailed);

        if let Ok(RedisValue::Integer(1)) = output {
            self.set_expiry(key, expiry).await?;
            return Ok(());
        }

        Err(AppError::SetExFailed)
    }

    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_expiry(&self, key: &str, seconds: i64) -> Result<(), AppError> {
        let output: Result<(), _> = self
            .pool
            .expire(key, seconds)
            .await
            .into_report()
            .change_context(AppError::SetExpiryFailed);

        if output.is_err() {
            return Err(AppError::SetExpiryFailed);
        }

        Ok(())
    }

    // get key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn get_key(&self, key: &str) -> Result<Option<String>, AppError> {
        let output: Result<RedisValue, _> = self
            .pool
            .get(key)
            .await
            .into_report()
            .change_context(AppError::GetFailed);

        match output {
            Ok(RedisValue::String(val)) => Ok(Some(val.to_string())),
            Ok(RedisValue::Null) => Ok(None),
            _ => Err(AppError::GetFailed),
        }
    }

    // delete key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn delete_key(&self, key: &str) -> Result<(), AppError> {
        let output: Result<(), _> = self
            .pool
            .del(key)
            .await
            .into_report()
            .change_context(AppError::DeleteFailed);

        if output.is_err() {
            return Err(AppError::DeleteFailed);
        }

        Ok(())
    }

    //HSET
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_hash_fields<V>(&self, key: &str, values: V) -> Result<(), AppError>
    where
        V: TryInto<RedisMap> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .hset(key, values)
            .await
            .into_report()
            .change_context(AppError::SetHashFieldFailed);

        // setting expiry for the key
        if output.is_ok() {
            self.set_expiry(key, self.config.default_hash_ttl.into())
                .await?;
        } else {
            return Err(AppError::SetHashFieldFailed);
        }

        Ok(())
    }

    //HGET
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn get_hash_field<V>(&self, key: &str, field: &str) -> Result<V, AppError>
    where
        V: FromRedis + Unpin + Send + 'static,
    {
        let output: Result<V, _> = self
            .pool
            .hget(key, field)
            .await
            .into_report()
            .change_context(AppError::GetHashFieldFailed);

        if output.is_err() {
            return Err(AppError::GetHashFieldFailed);
        }

        Ok(output.unwrap())
    }

    //RPUSH
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn rpush<V>(&self, key: &str, values: Vec<V>) -> Result<i64, AppError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output = self
            .pool
            .rpush(key, values)
            .await
            .into_report()
            .change_context(AppError::RPushFailed);

        if let Ok(RedisValue::Integer(length)) = output {
            Ok(length)
        } else {
            Err(AppError::RPushFailed)
        }
    }

    //RPOP
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn rpop(&self, key: &str, count: Option<usize>) -> Result<Vec<String>, AppError> {
        let output = self
            .pool
            .rpop(key, count)
            .await
            .into_report()
            .change_context(AppError::RPopFailed);

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
            _ => Err(AppError::RPopFailed),
        }
    }

    //LRANGE
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn lrange(&self, key: &str, min: i64, max: i64) -> Result<Vec<String>, AppError> {
        let output = self
            .pool
            .lrange(key, min, max)
            .await
            .into_report()
            .change_context(AppError::LRangeFailed);

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
            _ => Err(AppError::LRangeFailed),
        }
    }

    //LLEN
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn llen(&self, key: &str) -> Result<i64, AppError> {
        let output = self
            .pool
            .llen(key)
            .await
            .into_report()
            .change_context(AppError::RPushFailed);

        if let Ok(RedisValue::Integer(length)) = output {
            Ok(length)
        } else {
            Err(AppError::RPushFailed)
        }
    }

    //GEOADD
    #[instrument(level = "DEBUG", skip(self))]
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
        let output: Result<(), _> = self
            .pool
            .geoadd(key, options, changed, values)
            .await
            .into_report()
            .change_context(AppError::GeoAddFailed);

        if output.is_err() {
            return Err(AppError::GeoAddFailed);
        }

        Ok(())
    }

    //GEOADD with expiry
    #[instrument(level = "DEBUG", skip(self))]
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
        let output: Result<RedisValue, _> = self
            .pool
            .geoadd(key, options, changed, values)
            .await
            .into_report()
            .change_context(AppError::GeoAddFailed);

        if output.is_err() {
            return Err(AppError::GeoAddFailed);
        }

        if let Ok(RedisValue::Integer(1)) = output {
            self.set_expiry(key, expiry as i64).await?;
            return Ok(());
        }

        Err(AppError::SetExFailed)
    }

    //GEOSEARCH
    #[allow(clippy::too_many_arguments)]
    #[instrument(level = "DEBUG", skip(self))]
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
            .await
            .into_report()
            .change_context(AppError::GeoSearchFailed);

        if output.is_err() {
            return Err(AppError::GeoSearchFailed);
        }

        Ok(output.unwrap())
    }

    //GEOPOS
    pub async fn geopos(&self, key: &str, members: Vec<String>) -> Result<Vec<Point>, AppError> {
        let output: Result<RedisValue, _> = self
            .pool
            .geopos(key, members)
            .await
            .into_report()
            .change_context(AppError::GeoPosFailed);

        match output {
            Ok(RedisValue::Array(points)) => {
                if !points.is_empty() {
                    if points[0].is_array() {
                        let mut resp = Vec::new();
                        for point in points {
                            let point = point.as_geo_position().expect("Unable to parse point");
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
                            lat: points[1].as_f64().expect("Unable to parse lat"),
                            lon: points[0].as_f64().expect("Unable to parse lon"),
                        }]);
                    } else {
                        return Ok(vec![]);
                    }
                } else {
                    Ok(vec![])
                }
            }
            _ => Err(AppError::GeoPosFailed),
        }
    }

    //ZREMRANGEBYRANK
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn zremrange_by_rank(
        &self,
        key: &str,
        start: i64,
        stop: i64,
    ) -> Result<(), AppError> {
        let output: Result<(), _> = self
            .pool
            .zremrangebyrank(key, start, stop)
            .await
            .into_report()
            .change_context(AppError::ZremrangeByRankFailed);

        if output.is_err() {
            return Err(AppError::ZremrangeByRankFailed);
        }

        Ok(())
    }

    //ZADD
    #[instrument(level = "DEBUG", skip(self))]
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
            .await
            .into_report()
            .change_context(AppError::ZAddFailed);

        if output.is_err() {
            return Err(AppError::ZAddFailed);
        }

        Ok(())
    }

    //ZCARD
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn zcard(&self, key: &str) -> Result<u64, AppError> {
        let output: Result<u64, _> = self
            .pool
            .zcard(key)
            .await
            .into_report()
            .change_context(AppError::ZCardFailed);

        if output.is_err() {
            return Err(AppError::ZCardFailed);
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
            .await
            .into_report()
            .change_context(AppError::ZRangeFailed);

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
            _ => Err(AppError::ZRangeFailed),
        }
    }
}
