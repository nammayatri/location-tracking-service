use crate::errors;
use common_utils::errors::CustomResult;
use error_stack::{IntoReport, ResultExt};
use fred::{
    interfaces::{GeoInterface, KeysInterface, SortedSetsInterface},
    types::{Expiration, FromRedis, GeoPosition, GeoValue, RedisValue, SetOptions},
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

    //GEOADD
    #[instrument(level = "DEBUG", skip(self))]
    pub async fn geo_add(
        &self,
        key: &str,
        longitude: f64,
        latitude: f64,
        member: &str,
        options: Option<SetOptions>,
        changed: bool,
    ) -> CustomResult<(), errors::RedisError> {
        self.pool
            .geoadd(
                key,
                options,
                changed,
                vec![GeoValue::new(
                    GeoPosition::from((longitude, latitude)),
                    member,
                )],
            )
            .await
            .into_report()
            .change_context(errors::RedisError::GeoAddFailed)
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

    #[tokio::test]
    async fn test_set_key() {
        let is_success = tokio::task::spawn_blocking(move || {
            futures::executor::block_on(async {
                // Arrange
                let pool = RedisConnectionPool::new(&RedisSettings::default())
                    .await
                    .expect("Failed to create Redis Connection Pool");

                // Act

                // let result = pool.set_key("chakri", "value".to_string()).await;
                // print!("{:?}", pool.get_key::<String>("chakri").await);

                // let result = pool.geo_add("chakriGeo", 1.0, 2.0, "value", None, false).await;

                let result = pool.zremrange_by_rank("zremrange_by_rank_key", 0, 5).await;

                // Assert Setup
                result.is_ok()
            })
        })
        .await
        .expect("Spawn block failure");

        assert!(is_success);
    }
}
