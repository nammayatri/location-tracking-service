use crate::redis::client::{error, types::RedisConnectionPool};
use crate::utils::logger::instrument;
use error_stack::{IntoReport, ResultExt};
use fred::{
    interfaces::{GeoInterface, HashesInterface, KeysInterface, SortedSetsInterface},
    types::{
        Expiration, FromRedis, GeoPosition, GeoRadiusInfo, GeoUnit, Limit, MultipleGeoValues,
        Ordering, RedisMap, RedisValue, SetOptions, SortOrder, ZSort,
    },
};
use std::fmt::Debug;

impl RedisConnectionPool {
    // set key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_key<V>(&self, key: &str, value: V) -> Result<(), error::RedisError>
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
            .change_context(error::RedisError::SetFailed);

        if !output.is_ok() {
            return Err(error::RedisError::SetFailed.into());
        }

        Ok(())
    }

    // set key with expiry
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_with_expiry<V>(
        &self,
        key: &str,
        value: V,
        expiry: u32,
    ) -> Result<(), error::RedisError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .set(key, value, Some(Expiration::EX(expiry.into())), None, false)
            .await
            .into_report()
            .change_context(error::RedisError::SetFailed);

        if !output.is_ok() {
            return Err(error::RedisError::SetFailed.into());
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
    ) -> Result<(), error::RedisError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .msetnx((key, value))
            .await
            .into_report()
            .change_context(error::RedisError::SetFailed);

        if output.is_ok() {
            self.set_expiry(key, expiry).await?;
        } else {
            return Err(error::RedisError::SetFailed.into());
        }

        Ok(())
    }

    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_expiry(&self, key: &str, seconds: i64) -> Result<(), error::RedisError> {
        let output: Result<(), _> = self
            .pool
            .expire(key, seconds)
            .await
            .into_report()
            .change_context(error::RedisError::SetExpiryFailed);

        if !output.is_ok() {
            return Err(error::RedisError::SetExpiryFailed.into());
        }

        Ok(())
    }

    // get key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn get_key<V>(&self, key: &str) -> Result<V, error::RedisError>
    where
        V: FromRedis + Unpin + Send + 'static,
    {
        let output: Result<V, _> = self
            .pool
            .get(key)
            .await
            .into_report()
            .change_context(error::RedisError::GetFailed);

        if !output.is_ok() {
            return Err(error::RedisError::GetFailed.into());
        }

        Ok(output.unwrap())
    }

    // delete key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn delete_key(&self, key: &str) -> Result<(), error::RedisError> {
        let output: Result<(), _> = self
            .pool
            .del(key)
            .await
            .into_report()
            .change_context(error::RedisError::DeleteFailed);

        if !output.is_ok() {
            return Err(error::RedisError::DeleteFailed.into());
        }

        Ok(())
    }

    //HSET
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_hash_fields<V>(&self, key: &str, values: V) -> Result<(), error::RedisError>
    where
        V: TryInto<RedisMap> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .hset(key, values)
            .await
            .into_report()
            .change_context(error::RedisError::SetHashFailed);

        // setting expiry for the key
        if output.is_ok() {
            self.set_expiry(key, self.config.default_hash_ttl.into())
                .await?;
        } else {
            return Err(error::RedisError::SetHashFailed.into());
        }

        Ok(())
    }

    //HGET
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn get_hash_field<V>(&self, key: &str, field: &str) -> Result<V, error::RedisError>
    where
        V: FromRedis + Unpin + Send + 'static,
    {
        let output: Result<V, _> = self
            .pool
            .hget(key, field)
            .await
            .into_report()
            .change_context(error::RedisError::GetHashFieldFailed);

        if !output.is_ok() {
            return Err(error::RedisError::GetHashFieldFailed.into());
        }

        Ok(output.unwrap())
    }

    //GEOADD
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn geo_add<V>(
        &self,
        key: &str,
        values: V,
        options: Option<SetOptions>,
        changed: bool,
    ) -> Result<(), error::RedisError>
    where
        V: Into<MultipleGeoValues> + Send + Debug,
    {
        let output: Result<(), _> = self
            .pool
            .geoadd(key, options, changed, values)
            .await
            .into_report()
            .change_context(error::RedisError::GeoAddFailed);

        if !output.is_ok() {
            return Err(error::RedisError::GeoAddFailed.into());
        }

        Ok(())
    }

    //GEOSEARCH
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
    ) -> Result<Vec<GeoRadiusInfo>, error::RedisError> {
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
            .change_context(error::RedisError::GeoSearchFailed);

        if !output.is_ok() {
            return Err(error::RedisError::GeoSearchFailed.into());
        }

        Ok(output.unwrap())
    }

    //GEOPOS
    pub async fn geopos(
        &self,
        key: &str,
        members: Vec<String>,
    ) -> Result<RedisValue, error::RedisError> {
        let output: Result<RedisValue, _> = self
            .pool
            .geopos(key, members)
            .await
            .into_report()
            .change_context(error::RedisError::GeoPosFailed);

        if !output.is_ok() {
            return Err(error::RedisError::GeoPosFailed.into());
        }

        Ok(output.unwrap())
    }

    //ZREMRANGEBYRANK
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn zremrange_by_rank(
        &self,
        key: &str,
        start: i64,
        stop: i64,
    ) -> Result<(), error::RedisError> {
        let output: Result<(), _> = self
            .pool
            .zremrangebyrank(key, start, stop)
            .await
            .into_report()
            .change_context(error::RedisError::ZremrangeByRankFailed);

        if !output.is_ok() {
            return Err(error::RedisError::ZremrangeByRankFailed.into());
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
    ) -> Result<(), error::RedisError> {
        let output: Result<(), _> = self
            .pool
            .zadd(key, options, ordering, changed, incr, values)
            .await
            .into_report()
            .change_context(error::RedisError::ZAddFailed);

        if !output.is_ok() {
            return Err(error::RedisError::ZAddFailed.into());
        }

        Ok(())
    }

    //ZCARD
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn zcard(&self, key: &str) -> Result<u64, error::RedisError> {
        let output: Result<u64, _> = self
            .pool
            .zcard(key)
            .await
            .into_report()
            .change_context(error::RedisError::ZCardFailed);

        if !output.is_ok() {
            return Err(error::RedisError::ZCardFailed.into());
        }

        Ok(output.unwrap())
    }

    //ZRANGE
    pub async fn zrange(
        &self,
        key: &str,
        min: i64,
        max: i64,
        sort: Option<ZSort>,
        rev: bool,
        limit: Option<Limit>,
        withscores: bool,
    ) -> Result<RedisValue, error::RedisError> {
        let output: Result<RedisValue, _> = self
            .pool
            .zrange(key, min, max, sort, rev, limit, withscores)
            .await
            .into_report()
            .change_context(error::RedisError::ZRangeFailed);

        if !output.is_ok() {
            return Err(error::RedisError::ZRangeFailed.into());
        }

        Ok(output.unwrap())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use crate::redis::client::types::{RedisConnectionPool, RedisSettings};
    // use fred::types::{GeoPosition, GeoValue};
    #[tokio::test]
    async fn test_set_key() {
        let is_success = tokio::task::spawn_blocking(move || {
            futures::executor::block_on(async {
                // Arrange
                let pool = RedisConnectionPool::new(&RedisSettings::default())
                    .await
                    .expect("Failed to create Redis Connection Pool");
                // Act
                let result = pool.set_key("chakri", "value".to_string()).await;
                print!("{:?}", pool.get_key::<String>("chakri").await);
                // let result = pool
                //     .geo_add(
                //         "GeoAdd",
                //         vec![GeoValue::new(GeoPosition::from((1.0, 2.0)), "value")],
                //         None,
                //         false,
                //     )
                //     .await;
                // let result = pool.zremrange_by_rank("zremrange_by_rank_key", 0, 5).await;
                // let result = pool.setnx_with_expiry("key", "value".to_string(), 40).await;
                // Assert Setup
                result.is_ok()
            })
        })
        .await
        .expect("Spawn block failure");
        assert!(is_success);
    }
}
