use std::sync::{atomic, Arc};

use super::error;
use crate::utils::logger;
use error_stack::{IntoReport, ResultExt};
use fred::interfaces::ClientLike;
use serde::Deserialize;

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
        }
    }
}

impl RedisSettings {
    pub fn new(host: String, port: u16, pool_size: usize) -> Self {
        RedisSettings {
            host,
            port,
            cluster_enabled: false,
            cluster_urls: Vec::new(),
            use_legacy_version: false,
            pool_size,
            reconnect_max_attempts: 5,
            reconnect_delay: 1000,
            default_ttl: 3600,
            default_hash_ttl: 3600,
            stream_read_count: 100,
        }
    }
}

#[derive(Debug)]
pub enum RedisEntryId {
    UserSpecifiedID {
        milliseconds: String,
        sequence_number: String,
    },
    AutoGeneratedID,
    AfterLastID,
    /// Applicable only with consumer groups
    UndeliveredEntryID,
}

pub struct RedisConfig {
    pub default_ttl: u32,            // time to live
    _default_stream_read_count: u64, // number of messages to read from a stream
    pub default_hash_ttl: u32,       // time to live for a hash
}

impl From<&RedisSettings> for RedisConfig {
    fn from(config: &RedisSettings) -> Self {
        Self {
            default_ttl: config.default_ttl,
            _default_stream_read_count: config.stream_read_count,
            default_hash_ttl: config.default_hash_ttl,
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
    ) -> Result<Self, error::RedisError> {
        let client = fred::prelude::RedisClient::new(config, None, Some(reconnect_policy));
        client.connect();
        client
            .wait_for_connect()
            .await
            .into_report()
            .change_context(error::RedisError::RedisConnectionError)
            .unwrap();
        Ok(Self { inner: client })
    }
}

pub struct RedisConnectionPool {
    pub pool: fred::pool::RedisPool,
    pub config: RedisConfig,
    join_handles: Vec<fred::types::ConnectHandle>,
    pub subscriber: RedisClient,
    pub publisher: RedisClient,
    pub is_redis_available: Arc<atomic::AtomicBool>,
}

impl RedisConnectionPool {
    /// Create a new Redis connection
    pub async fn new(conf: &RedisSettings) -> Result<Self, error::RedisError> {
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
                "redis://{}:{}", //URI Schema
                conf.host, conf.port,
            ),
        };
        let mut config = fred::types::RedisConfig::from_url(&redis_connection_url)
            .into_report()
            .change_context(error::RedisError::RedisConnectionError)
            .unwrap();

        if !conf.use_legacy_version {
            config.version = fred::types::RespVersion::RESP3;
        }
        config.tracing = fred::types::TracingConfig::new(true);
        config.blocking = fred::types::Blocking::Error;
        let reconnect_policy = fred::types::ReconnectPolicy::new_constant(
            conf.reconnect_max_attempts,
            conf.reconnect_delay,
        );

        let subscriber = RedisClient::new(config.clone(), reconnect_policy.clone()).await?;

        let publisher = RedisClient::new(config.clone(), reconnect_policy.clone()).await?;

        let pool = fred::pool::RedisPool::new(config, None, Some(reconnect_policy), conf.pool_size)
            .into_report()
            .change_context(error::RedisError::RedisConnectionError)
            .unwrap();

        let join_handles = pool.connect();
        pool.wait_for_connect()
            .await
            .into_report()
            .change_context(error::RedisError::RedisConnectionError)
            .unwrap();

        let config = RedisConfig::from(conf);

        Ok(Self {
            pool,
            config,
            join_handles,
            is_redis_available: Arc::new(atomic::AtomicBool::new(true)),
            subscriber,
            publisher,
        })
    }

    pub async fn close_connections(&mut self) {
        self.pool.quit_pool().await;
        for handle in self.join_handles.drain(..) {
            match handle.await {
                Ok(Ok(_)) => (),
                Ok(Err(error)) => logger::error!(%error),
                Err(error) => logger::error!(%error),
            };
        }
    }
    pub async fn on_error(&self) {
        while let Ok(redis_error) = self.pool.on_error().recv().await {
            logger::error!(?redis_error, "Redis protocol or connection error");
            if self.pool.state() == fred::types::ClientState::Disconnected {
                self.is_redis_available
                    .store(false, atomic::Ordering::SeqCst);
            }
        }
    }
}
