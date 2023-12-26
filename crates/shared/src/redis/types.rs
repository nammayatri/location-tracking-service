/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use std::sync::{atomic, Arc};

use error_stack::IntoReport;
use fred::interfaces::ClientLike;
use serde::Deserialize;
use tracing::error;

use super::error::RedisError;

#[derive(Debug)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct RedisSettings {
    pub host: String,
    pub port: u16,
    pub cluster_enabled: bool,
    pub cluster_urls: Vec<String>,
    pub use_legacy_version: bool,
    pub pool_size: usize,
    pub reconnect_max_attempts: u32,
    /// Reconnect delay in milliseconds
    pub reconnect_delay: u32,
    /// TTL in seconds
    pub default_ttl: u32,
    /// TTL for hash-tables in seconds
    pub default_hash_ttl: u32,
    pub stream_read_count: u64,
    pub partition: usize,
}

impl Default for RedisSettings {
    fn default() -> Self {
        RedisSettings {
            host: String::from("localhost"),
            port: 6379,
            cluster_enabled: false,
            cluster_urls: Vec::new(),
            use_legacy_version: false,
            pool_size: 10,
            reconnect_max_attempts: 5,
            reconnect_delay: 1000,
            default_ttl: 3600,
            default_hash_ttl: 3600,
            stream_read_count: 100,
            partition: 0,
        }
    }
}

impl RedisSettings {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        host: String,
        port: u16,
        pool_size: usize,
        partition: usize,
        reconnect_max_attempts: u32,
        reconnect_delay: u32,
        default_ttl: u32,
        default_hash_ttl: u32,
        stream_read_count: u64,
    ) -> Self {
        RedisSettings {
            host,
            port,
            partition,
            cluster_enabled: false,
            cluster_urls: Vec::new(),
            use_legacy_version: false,
            pool_size,
            reconnect_max_attempts,
            reconnect_delay,
            default_ttl,
            default_hash_ttl,
            stream_read_count,
        }
    }
}

pub struct RedisClient {
    inner: fred::prelude::RedisClient,
}

impl std::ops::Deref for RedisClient {
    type Target = fred::prelude::RedisClient;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl RedisClient {
    pub async fn new(
        config: fred::types::RedisConfig,
        reconnect_policy: fred::types::ReconnectPolicy,
    ) -> Result<Self, RedisError> {
        let client = fred::prelude::RedisClient::new(config, None, Some(reconnect_policy));
        client.connect();
        client
            .wait_for_connect()
            .await
            .map_err(|err| RedisError::RedisConnectionError(err.to_string()))?;
        Ok(Self { inner: client })
    }
}

pub struct RedisConnectionPool {
    pub pool: fred::pool::RedisPool,
    pub migration_pool: Option<fred::pool::RedisPool>,
    join_handles: Vec<fred::types::ConnectHandle>,
    is_redis_available: Arc<atomic::AtomicBool>,
}

impl RedisConnectionPool {
    /// Create a new Redis connection
    pub async fn new(
        conf: RedisSettings,
        migration_conf: Option<RedisSettings>,
    ) -> Result<Self, RedisError> {
        let (pool, mut join_handles) = Self::instantiate(&conf).await?;

        if let Some(migration_conf) = migration_conf {
            let (migration_pool, migration_join_handles) =
                Self::instantiate(&migration_conf).await?;
            join_handles.extend(migration_join_handles);
            Ok(Self {
                pool,
                migration_pool: Some(migration_pool),
                join_handles,
                is_redis_available: Arc::new(atomic::AtomicBool::new(true)),
            })
        } else {
            Ok(Self {
                pool,
                migration_pool: None,
                join_handles,
                is_redis_available: Arc::new(atomic::AtomicBool::new(true)),
            })
        }
    }
    async fn instantiate(
        conf: &RedisSettings,
    ) -> Result<(fred::pool::RedisPool, Vec<fred::types::ConnectHandle>), RedisError> {
        let redis_connection_url = match conf.cluster_enabled {
            // Fred relies on this format for specifying cluster where the host port is ignored & only query parameters are used for node addresses
            // redis-cluster://username:password@host:port?node=bar.com:30002&node=baz.com:30003
            true => format!(
                "redis-cluster://{}:{}?{}",
                conf.host,
                conf.port,
                conf.cluster_urls
                    .iter()
                    .flat_map(|url| vec!["&", url])
                    .skip(1)
                    .collect::<String>()
            ),
            false => format!(
                "redis://{}:{}/{}", //URI Schema
                conf.host, conf.port, conf.partition
            ),
        };
        let mut config = fred::types::RedisConfig::from_url(&redis_connection_url)
            .into_report()
            .map_err(|err| RedisError::RedisConnectionError(err.to_string()))?;

        if !conf.use_legacy_version {
            config.version = fred::types::RespVersion::RESP3;
        }
        config.tracing = fred::types::TracingConfig::new(true);
        config.blocking = fred::types::Blocking::Error;
        let reconnect_policy = fred::types::ReconnectPolicy::new_constant(
            conf.reconnect_max_attempts,
            conf.reconnect_delay,
        );

        let pool = fred::pool::RedisPool::new(config, None, Some(reconnect_policy), conf.pool_size)
            .into_report()
            .map_err(|err| RedisError::RedisConnectionError(err.to_string()))?;

        let join_handles = pool.connect();
        pool.wait_for_connect()
            .await
            .into_report()
            .map_err(|err| RedisError::RedisConnectionError(err.to_string()))?;

        Ok((pool, join_handles))
    }

    pub async fn close_connections(&mut self) {
        self.pool.quit_pool().await;
        for handle in self.join_handles.drain(..) {
            match handle.await {
                Ok(Ok(_)) => (),
                Ok(Err(error)) => error!(%error),
                Err(error) => error!(%error),
            };
        }
    }
    pub async fn on_error(&self) {
        while let Ok(redis_error) = self.pool.on_error().recv().await {
            error!(?redis_error, "Redis protocol or connection error");
            if self.pool.state() == fred::types::ClientState::Disconnected {
                self.is_redis_available
                    .store(false, atomic::Ordering::SeqCst);
            }
        }
    }
}
