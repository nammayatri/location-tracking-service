/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use rdkafka::{error::KafkaError, producer::FutureProducer, ClientConfig};
use serde::Deserialize;
use shared::redis::types::{RedisConnectionPool, RedisSettings};
use std::sync::Arc;
use tracing::info;

use crate::tools::logger::LoggerConfig;

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub kafka_key: String,
    pub kafka_host: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub prometheus_port: u16,
    pub logger_cfg: LoggerConfig,
    pub redis_cfg: RedisConfig,
    pub kafka_cfg: KafkaConfig,
    pub reader_delay_seconds: u64,
    pub retry_delay_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub redis_host: String,
    pub redis_port: u16,
    pub redis_pool_size: usize,
    pub redis_partition: usize,
    pub reconnect_max_attempts: u32,
    pub reconnect_delay: u32,
    pub default_ttl: u32,
    pub default_hash_ttl: u32,
    pub stream_read_count: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub redis_pool: Arc<RedisConnectionPool>,
    pub reader_delay_seconds: u64,
    pub retry_delay_seconds: u64,
    pub producer: Option<FutureProducer>,
    pub port: u16,
    pub prometheus_port: u16,
}

impl AppState {
    pub async fn new(app_config: AppConfig) -> AppState {
        let redis_pool = Arc::new(
            RedisConnectionPool::new(
                RedisSettings::new(
                    app_config.redis_cfg.redis_host,
                    app_config.redis_cfg.redis_port,
                    app_config.redis_cfg.redis_pool_size,
                    app_config.redis_cfg.redis_partition,
                    app_config.redis_cfg.reconnect_max_attempts,
                    app_config.redis_cfg.reconnect_delay,
                    app_config.redis_cfg.default_ttl,
                    app_config.redis_cfg.default_hash_ttl,
                    app_config.redis_cfg.stream_read_count,
                ),
                None,
            )
            .await
            .expect("Failed to create Redis connection pool"),
        );

        let producer: Option<FutureProducer>;

        let result: Result<FutureProducer, KafkaError> = ClientConfig::new()
            .set(
                app_config.kafka_cfg.kafka_key,
                app_config.kafka_cfg.kafka_host,
            )
            .set("compression.type", "lz4")
            .create();

        match result {
            Ok(val) => {
                producer = Some(val);
            }
            Err(err) => {
                producer = None;
                info!(
                    tag = "[Kafka Connection]",
                    "Error connecting to kafka config: {err}"
                );
            }
        }

        AppState {
            redis_pool,
            reader_delay_seconds: app_config.reader_delay_seconds,
            retry_delay_seconds: app_config.retry_delay_seconds,
            producer,
            port: app_config.port,
            prometheus_port: app_config.prometheus_port,
        }
    }
}
