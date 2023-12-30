/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use std::{env::var, sync::Arc};

use crate::{
    common::{geo_polygon::read_geo_polygon, types::*},
    tools::logger::LoggerConfig,
};
use rdkafka::{error::KafkaError, producer::FutureProducer, ClientConfig};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::tools::error::AppError;
use shared::{
    redis::types::{RedisConnectionPool, RedisSettings},
    utils::logger::*,
};
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tracing::info;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub logger_cfg: LoggerConfig,
    pub persistent_redis_cfg: RedisConfig,
    pub non_persistent_redis_cfg: RedisConfig,
    pub persistent_migration_redis_cfg: RedisConfig,
    pub non_persistent_migration_redis_cfg: RedisConfig,
    pub redis_migration_stage: bool,
    pub workers: usize,
    pub drainer_delay: u64,
    pub drainer_size: usize,
    pub new_ride_drainer_delay: u64,
    pub auth_url: String,
    pub auth_api_key: String,
    pub bulk_location_callback_url: String,
    pub auth_token_expiry: u32,
    pub redis_expiry: u32,
    pub min_location_accuracy: f64,
    pub last_location_timstamp_expiry: u32,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub kafka_cfg: KafkaConfig,
    pub driver_location_update_topic: String,
    pub batch_size: i64,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
    pub driver_location_accuracy_buffer: f64,
    pub blacklist_merchants: Vec<String>,
    pub request_timeout: u64,
    pub log_unprocessible_req_body: Vec<String>,
    pub max_allowed_req_size: usize,
    pub cac_config: CacConfig,
    pub superposition_client_config: SuperpositionClientConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KafkaConfig {
    pub kafka_key: String,
    pub kafka_host: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CacConfig {
    pub cac_hostname: String,
    pub cac_polling_interval: Duration,
    pub update_cac_periodically: bool,
    pub cac_tenants: Vec<String>,
}
#[derive(Debug, Deserialize, Serialize, Clone)]

pub struct SuperpositionClientConfig {
    pub superposition_hostname: String,
    pub superposition_poll_frequency: u64,
}
#[derive(Debug, Deserialize, Serialize, Clone)]
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

impl AppConfig {
    pub fn get_field(&self, key: &str) -> Result<Value, AppError> {
        match key {
            "port" => Ok(Value::Number(serde_json::Number::from(self.port))),
            "logger_cfg" => {
                let res = serde_json::to_value(self.logger_cfg);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "persistent_redis_cfg" => {
                let res = serde_json::to_value(&self.persistent_redis_cfg);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "non_persistent_redis_cfg" => {
                let res = serde_json::to_value(&self.non_persistent_redis_cfg);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "persistent_migration_redis_cfg" => {
                let res = serde_json::to_value(&self.persistent_migration_redis_cfg);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "non_persistent_migration_redis_cfg" => {
                let res = serde_json::to_value(&self.non_persistent_migration_redis_cfg);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "redis_migration_stage" => Ok(Value::Bool(self.redis_migration_stage)),
            "workers" => Ok(Value::Number(serde_json::Number::from(self.workers))),
            "drainer_delay" => Ok(Value::Number(serde_json::Number::from(self.drainer_delay))),
            "drainer_size" => Ok(Value::Number(serde_json::Number::from(self.drainer_size))),
            "new_ride_drainer_delay" => Ok(Value::Number(serde_json::Number::from(
                self.new_ride_drainer_delay,
            ))),
            "auth_url" => Ok(Value::String(self.auth_url.clone())),
            "auth_api_key" => Ok(Value::String(self.auth_api_key.clone())),
            "bulk_location_callback_url" => {
                Ok(Value::String(self.bulk_location_callback_url.clone()))
            }
            "auth_token_expiry" => Ok(Value::Number(serde_json::Number::from(
                self.auth_token_expiry,
            ))),
            "redis_expiry" => Ok(Value::Number(serde_json::Number::from(self.redis_expiry))),
            "min_location_accuracy" => {
                let res = serde_json::Number::from_f64(self.min_location_accuracy);
                match res {
                    Some(res) => Ok(Value::Number(res)),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config due to failure in decoding f64 to Number"
                            .to_string(),
                    )),
                }
            }
            "last_location_timstamp_expiry" => Ok(Value::Number(serde_json::Number::from(
                self.last_location_timstamp_expiry,
            ))),
            "location_update_limit" => Ok(Value::Number(serde_json::Number::from(
                self.location_update_limit,
            ))),
            "location_update_interval" => Ok(Value::Number(serde_json::Number::from(
                self.location_update_interval,
            ))),
            "kafka_cfg" => {
                let res = serde_json::to_value(&self.kafka_cfg);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "driver_location_update_topic" => {
                Ok(Value::String(self.driver_location_update_topic.clone()))
            }
            "batch_size" => Ok(Value::Number(serde_json::Number::from(self.batch_size))),
            "bucket_size" => Ok(Value::Number(serde_json::Number::from(self.bucket_size))),
            "nearby_bucket_threshold" => Ok(Value::Number(serde_json::Number::from(
                self.nearby_bucket_threshold,
            ))),
            "driver_location_accuracy_buffer" => {
                let res = serde_json::Number::from_f64(self.driver_location_accuracy_buffer);
                match res {
                    Some(res) => Ok(Value::Number(res)),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config due to failure in decoding f64 to Number"
                            .to_string(),
                    )),
                }
            }
            "blacklist_merchants" => {
                let res = serde_json::to_value(&self.blacklist_merchants);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "request_timeout" => Ok(Value::Number(serde_json::Number::from(
                self.request_timeout,
            ))),
            "log_unprocessible_req_body" => {
                let res = serde_json::to_value(&self.log_unprocessible_req_body);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "max_allowed_req_size" => Ok(Value::Number(serde_json::Number::from(
                self.max_allowed_req_size,
            ))),
            "cac_config" => {
                let res = serde_json::to_value(&self.cac_config);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            "superposition_client_config" => {
                let res = serde_json::to_value(&self.superposition_client_config);
                match res {
                    Ok(res) => Ok(res),
                    _ => Err(AppError::DefaultConfigsNotFound(
                        "Failed to extract default config".to_string(),
                    )),
                }
            }
            _ => Err(AppError::DefaultConfigsNotFound(
                "Failed to extract default config, given key did not match any default config's field".to_string(),
            )),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub non_persistent_redis: Arc<RedisConnectionPool>,
    pub persistent_redis: Arc<RedisConnectionPool>,
    pub sender: Sender<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    pub drainer_delay: u64,
    pub drainer_size: usize,
    pub new_ride_drainer_delay: u64,
    pub polygon: Vec<MultiPolygonBody>,
    pub blacklist_polygon: Vec<MultiPolygonBody>,
    pub auth_url: Url,
    pub auth_api_key: String,
    pub bulk_location_callback_url: Url,
    pub auth_token_expiry: u32,
    pub redis_expiry: u32,
    pub min_location_accuracy: Accuracy,
    pub last_location_timstamp_expiry: u32,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub producer: Option<FutureProducer>,
    pub driver_location_update_topic: String,
    pub batch_size: i64,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
    pub driver_location_accuracy_buffer: f64,
    pub blacklist_merchants: Vec<MerchantId>,
    pub max_allowed_req_size: usize,
    pub log_unprocessible_req_body: Vec<String>,
    pub request_timeout: u64,
}

impl AppState {
    pub async fn new(
        app_config: AppConfig,
        sender: Sender<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    ) -> AppState {
        let non_persistent_migration_redis_settings = if app_config.redis_migration_stage {
            Some(RedisSettings::new(
                app_config.non_persistent_migration_redis_cfg.redis_host,
                app_config.non_persistent_migration_redis_cfg.redis_port,
                app_config
                    .non_persistent_migration_redis_cfg
                    .redis_pool_size,
                app_config
                    .non_persistent_migration_redis_cfg
                    .redis_partition,
                app_config
                    .non_persistent_migration_redis_cfg
                    .reconnect_max_attempts,
                app_config
                    .non_persistent_migration_redis_cfg
                    .reconnect_delay,
                app_config.non_persistent_migration_redis_cfg.default_ttl,
                app_config
                    .non_persistent_migration_redis_cfg
                    .default_hash_ttl,
                app_config
                    .non_persistent_migration_redis_cfg
                    .stream_read_count,
            ))
        } else {
            None
        };

        let non_persistent_redis = Arc::new(
            RedisConnectionPool::new(
                RedisSettings::new(
                    app_config.non_persistent_redis_cfg.redis_host,
                    app_config.non_persistent_redis_cfg.redis_port,
                    app_config.non_persistent_redis_cfg.redis_pool_size,
                    app_config.non_persistent_redis_cfg.redis_partition,
                    app_config.non_persistent_redis_cfg.reconnect_max_attempts,
                    app_config.non_persistent_redis_cfg.reconnect_delay,
                    app_config.non_persistent_redis_cfg.default_ttl,
                    app_config.non_persistent_redis_cfg.default_hash_ttl,
                    app_config.non_persistent_redis_cfg.stream_read_count,
                ),
                non_persistent_migration_redis_settings,
            )
            .await
            .expect("Failed to create Location Redis connection pool"),
        );

        let persistent_migration_redis_settings = if app_config.redis_migration_stage {
            Some(RedisSettings::new(
                app_config.persistent_migration_redis_cfg.redis_host,
                app_config.persistent_migration_redis_cfg.redis_port,
                app_config.persistent_migration_redis_cfg.redis_pool_size,
                app_config.persistent_migration_redis_cfg.redis_partition,
                app_config
                    .persistent_migration_redis_cfg
                    .reconnect_max_attempts,
                app_config.persistent_migration_redis_cfg.reconnect_delay,
                app_config.persistent_migration_redis_cfg.default_ttl,
                app_config.persistent_migration_redis_cfg.default_hash_ttl,
                app_config.persistent_migration_redis_cfg.stream_read_count,
            ))
        } else {
            None
        };

        let persistent_redis = Arc::new(
            RedisConnectionPool::new(
                RedisSettings::new(
                    app_config.persistent_redis_cfg.redis_host,
                    app_config.persistent_redis_cfg.redis_port,
                    app_config.persistent_redis_cfg.redis_pool_size,
                    app_config.persistent_redis_cfg.redis_partition,
                    app_config.persistent_redis_cfg.reconnect_max_attempts,
                    app_config.persistent_redis_cfg.reconnect_delay,
                    app_config.persistent_redis_cfg.default_ttl,
                    app_config.persistent_redis_cfg.default_hash_ttl,
                    app_config.persistent_redis_cfg.stream_read_count,
                ),
                persistent_migration_redis_settings,
            )
            .await
            .expect("Failed to create Generic Redis connection pool"),
        );

        let geo_config_path = var("GEO_CONFIG").unwrap_or_else(|_| "./geo_config".to_string());
        let polygons = read_geo_polygon(&geo_config_path).expect("Failed to read geoJSON");

        let blacklist_geo_config_path =
            var("BLACKLIST_GEO_CONFIG").unwrap_or_else(|_| "./blacklist_geo_config".to_string());
        let blacklist_polygons = read_geo_polygon(&blacklist_geo_config_path)
            .expect("Failed to read specialzone geoJSON");

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

        let blacklist_merchants = app_config
            .blacklist_merchants
            .into_iter()
            .map(MerchantId)
            .collect::<Vec<MerchantId>>();

        AppState {
            non_persistent_redis,
            persistent_redis,
            drainer_delay: app_config.drainer_delay,
            drainer_size: app_config.drainer_size,
            new_ride_drainer_delay: app_config.new_ride_drainer_delay,
            sender,
            polygon: polygons,
            blacklist_polygon: blacklist_polygons,
            auth_url: Url::parse(app_config.auth_url.as_str()).expect("Failed to parse auth_url."),
            auth_api_key: app_config.auth_api_key,
            bulk_location_callback_url: Url::parse(app_config.bulk_location_callback_url.as_str())
                .expect("Failed to parse bulk_location_callback_url."),
            auth_token_expiry: app_config.auth_token_expiry,
            min_location_accuracy: Accuracy(app_config.min_location_accuracy),
            redis_expiry: app_config.redis_expiry,
            last_location_timstamp_expiry: app_config.last_location_timstamp_expiry,
            location_update_limit: app_config.location_update_limit,
            location_update_interval: app_config.location_update_interval,
            producer,
            driver_location_update_topic: app_config.driver_location_update_topic,
            batch_size: app_config.batch_size,
            bucket_size: app_config.bucket_size,
            nearby_bucket_threshold: app_config.nearby_bucket_threshold,
            driver_location_accuracy_buffer: app_config.driver_location_accuracy_buffer,
            max_allowed_req_size: app_config.max_allowed_req_size,
            log_unprocessible_req_body: app_config.log_unprocessible_req_body,
            request_timeout: app_config.request_timeout,
            blacklist_merchants,
        }
    }
}
