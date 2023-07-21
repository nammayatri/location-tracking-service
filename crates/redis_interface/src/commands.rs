use crate::errors;
use common_utils::errors::CustomResult;
use error_stack::{IntoReport, ResultExt};
use fred::{
    interfaces::{GeoInterface, HashesInterface, KeysInterface, SortedSetsInterface},
    types::{
        Expiration, FromRedis, GeoPosition, GeoRadiusInfo, GeoUnit, GeoValue, MultipleGeoValues,
        RedisMap, RedisValue, SetOptions, SortOrder,
    },
};
use router_env::{instrument, tracing};
use std::fmt::Debug;

impl super::RedisConnectionPool {
    // set key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_key<V>(&self, key: &str, value: V) -> CustomResult<(), errors::RedisError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        self.pool
            .set(
                key,
                value,
                Some(Expiration::EX(self.config.default_ttl.into())),
                None,
                false,
            )
            .await
            .into_report()
            .change_context(errors::RedisError::SetFailed)
    }

    // set key with expiry
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_with_expiry<V>(
        &self,
        key: &str,
        value: V,
        expiry: u32,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        self.pool
            .set(key, value, Some(Expiration::EX(expiry.into())), None, false)
            .await
            .into_report()
            .change_context(errors::RedisError::SetFailed)
    }

    // setnx key with expiry
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn setnx_with_expiry<V>(
        &self,
        key: &str,
        value: V,
        expiry: i64,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: TryInto<RedisValue> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .msetnx((key, value))
            .await
            .into_report()
            .change_context(errors::RedisError::SetFailed);

        if output.is_ok() {
            self.set_expiry(key, expiry).await?;
        } else {
            return Err(errors::RedisError::SetFailed.into());
        }

        Ok(())
    }

    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_expiry(
        &self,
        key: &str,
        seconds: i64,
    ) -> CustomResult<(), errors::RedisError> {
        self.pool
            .expire(key, seconds)
            .await
            .into_report()
            .change_context(errors::RedisError::SetExpiryFailed)
    }

    // get key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn get_key<V>(&self, key: &str) -> CustomResult<V, errors::RedisError>
    where
        V: FromRedis + Unpin + Send + 'static,
    {
        self.pool
            .get(key)
            .await
            .into_report()
            .change_context(errors::RedisError::GetFailed)
    }

    // delete key
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn delete_key(&self, key: &str) -> CustomResult<(), errors::RedisError> {
        self.pool
            .del(key)
            .await
            .into_report()
            .change_context(errors::RedisError::DeleteFailed)
    }

    //HSET
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn set_hash_fields<V>(
        &self,
        key: &str,
        values: V,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: TryInto<RedisMap> + Debug + Send + Sync,
        V::Error: Into<fred::error::RedisError> + Send + Sync,
    {
        let output: Result<(), _> = self
            .pool
            .hset(key, values)
            .await
            .into_report()
            .change_context(errors::RedisError::SetHashFailed);

        // setting expiry for the key
        if output.is_ok() {
            self.set_expiry(key, self.config.default_hash_ttl.into())
                .await?;
        } else {
            return Err(errors::RedisError::SetHashFailed.into());
        }

        Ok(())
    }

    //HGET
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn get_hash_field<V>(
        &self,
        key: &str,
        field: &str,
    ) -> CustomResult<V, errors::RedisError>
    where
        V: FromRedis + Unpin + Send + 'static,
    {
        self.pool
            .hget(key, field)
            .await
            .into_report()
            .change_context(errors::RedisError::GetHashFieldFailed)
    }

    //GEOADD
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn geo_add<V>(
        &self,
        key: &str,
        values: V,
        options: Option<SetOptions>,
        changed: bool,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: Into<MultipleGeoValues> + Send + Debug,
    {
        self.pool
            .geoadd(key, options, changed, values)
            .await
            .into_report()
            .change_context(errors::RedisError::GeoAddFailed)
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
    ) -> CustomResult<Vec<GeoRadiusInfo>, errors::RedisError> {
        self.pool
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
            .change_context(errors::RedisError::GeoSearchFailed)
    }

    //ZREMRANGEBYRANK
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn zremrange_by_rank(
        &self,
        key: &str,
        start: i64,
        stop: i64,
    ) -> CustomResult<(), errors::RedisError> {
        self.pool
            .zremrangebyrank(key, start, stop)
            .await
            .into_report()
            .change_context(errors::RedisError::ZremrangeByRankFailed)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use crate::{RedisConnectionPool, RedisSettings};
    use fred::types::{GeoPosition, GeoValue};

    #[tokio::test]
    async fn test_set_key() {
        let is_success = tokio::task::spawn_blocking(move || {
            futures::executor::block_on(async {
                // Arrange
                let pool = RedisConnectionPool::new(&RedisSettings::default())
                    .await
                    .expect("Failed to create Redis Connection Pool");

                // Act

                let result = pool
                    .set_with_expiry("chakri", "value".to_string(), 10)
                    .await;
                // print!("{:?}", pool.get_key::<String>("chakri").await);

                // let result = pool
                //     .geo_add(
                //         "GeoAdd",
                //         vec![GeoValue::new(GeoPosition::from((1.0, 2.0)), "value"), GeoValue::new(GeoPosition::from((3.0, 4.0)), "value2")],
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

// GeoValue::new(
//     GeoPosition::from((longitude, latitude)),
//     member,
// )]
