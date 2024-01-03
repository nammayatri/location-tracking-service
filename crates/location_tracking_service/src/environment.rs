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
    common::{cac::get_config, geo_polygon::read_geo_polygon, types::*},
    tools::logger::LoggerConfig,
};
use rdkafka::{error::KafkaError, producer::FutureProducer, ClientConfig};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use shared::redis::types::{RedisConnectionPool, RedisSettings};
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
    pub redis_expiry: u32,
    pub kafka_cfg: KafkaConfig,
    pub driver_location_update_topic: String,
    pub blacklist_merchants: Vec<String>,
    pub request_timeout: u64,
    pub log_unprocessible_req_body: Vec<String>,
    pub max_allowed_req_size: usize,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
    pub cac_config: CacConfig,
    pub superposition_client_config: SuperpositionClientConfig,
    pub business_configs: BusinessConfigs,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BusinessConfigs {
    pub auth_token_expiry: u32,
    pub min_location_accuracy: f64,
    pub driver_location_accuracy_buffer: f64,
    pub last_location_timstamp_expiry: u32,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub batch_size: i64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KafkaConfig {
    pub kafka_key: String,
    pub kafka_host: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CacConfig {
    pub cac_hostname: String,
    pub cac_polling_interval: u64,
    pub update_cac_periodically: bool,
    pub cac_tenant: String,
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
    pub redis_expiry: u32,
    pub producer: Option<FutureProducer>,
    pub driver_location_update_topic: String,
    pub blacklist_merchants: Vec<MerchantId>,
    pub max_allowed_req_size: usize,
    pub log_unprocessible_req_body: Vec<String>,
    pub request_timeout: u64,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
    pub cac_tenant: String,
    pub business_configs: BusinessConfigs,
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
            redis_expiry: app_config.redis_expiry,
            producer,
            driver_location_update_topic: app_config.driver_location_update_topic,
            max_allowed_req_size: app_config.max_allowed_req_size,
            log_unprocessible_req_body: app_config.log_unprocessible_req_body,
            request_timeout: app_config.request_timeout,
            blacklist_merchants,
            bucket_size: app_config.bucket_size,
            nearby_bucket_threshold: app_config.nearby_bucket_threshold,
            cac_tenant: app_config.cac_config.cac_tenant,
            business_configs: app_config.business_configs,
        }
    }
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        info!("Cloning AppState");
        let business_configs = futures::executor::block_on(async {
            BusinessConfigs {
                auth_token_expiry: get_config(self.cac_tenant.clone(), "auth_token_expiry")
                    .await
                    .unwrap_or(self.business_configs.auth_token_expiry),
                min_location_accuracy: get_config(self.cac_tenant.clone(), "min_location_accuracy")
                    .await
                    .map_err(|err| {
                        log::error!("Error fetching min_location_accuracy: {}", err.message());
                    })
                    .unwrap_or(self.business_configs.min_location_accuracy),
                last_location_timstamp_expiry: get_config(
                    self.cac_tenant.clone(),
                    "last_location_timstamp_expiry",
                )
                .await
                .unwrap_or(self.business_configs.last_location_timstamp_expiry),
                location_update_limit: get_config(self.cac_tenant.clone(), "location_update_limit")
                    .await
                    .unwrap_or(self.business_configs.location_update_limit),
                location_update_interval: get_config(
                    self.cac_tenant.clone(),
                    "location_update_interval",
                )
                .await
                .unwrap_or(self.business_configs.location_update_interval),
                batch_size: get_config(self.cac_tenant.clone(), "batch_size")
                    .await
                    .unwrap_or(self.business_configs.batch_size),
                driver_location_accuracy_buffer: get_config(
                    self.cac_tenant.clone(),
                    "driver_location_accuracy_buffer",
                )
                .await
                .unwrap_or(self.business_configs.driver_location_accuracy_buffer),
            }
        });
        AppState {
            non_persistent_redis: self.non_persistent_redis.clone(),
            persistent_redis: self.persistent_redis.clone(),
            drainer_delay: self.drainer_delay,
            drainer_size: self.drainer_size,
            new_ride_drainer_delay: self.new_ride_drainer_delay,
            sender: self.sender.clone(),
            polygon: self.polygon.clone(),
            blacklist_polygon: self.blacklist_polygon.clone(),
            auth_url: self.auth_url.clone(),
            auth_api_key: self.auth_api_key.clone(),
            bulk_location_callback_url: self.bulk_location_callback_url.clone(),
            redis_expiry: self.redis_expiry,
            producer: self.producer.clone(),
            driver_location_update_topic: self.driver_location_update_topic.clone(),
            max_allowed_req_size: self.max_allowed_req_size,
            log_unprocessible_req_body: self.log_unprocessible_req_body.clone(),
            request_timeout: self.request_timeout,
            blacklist_merchants: self.blacklist_merchants.clone(),
            bucket_size: self.bucket_size,
            nearby_bucket_threshold: self.nearby_bucket_threshold,
            cac_tenant: self.cac_tenant.clone(),
            business_configs,
        }
    }
}
