use std::{env::var, sync::Arc};

use rdkafka::{error::KafkaError, producer::FutureProducer, ClientConfig};
use serde::Deserialize;
use shared::{
    redis::types::{RedisConnectionPool, RedisSettings},
    utils::logger::*,
};
use tokio::sync::mpsc::Sender;

use crate::common::{geo_polygon::read_geo_polygon, types::*};

#[derive(Debug, Deserialize, Clone)]
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub kafka_key: String,
    pub kafka_host: String,
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
    pub non_persistent_redis: Arc<RedisConnectionPool>,
    pub persistent_redis: Arc<RedisConnectionPool>,
    pub sender: Sender<(Dimensions, Latitude, Longitude, DriverId)>,
    pub drainer_delay: u64,
    pub drainer_size: usize,
    pub new_ride_drainer_delay: u64,
    pub polygon: Vec<MultiPolygonBody>,
    pub auth_url: String,
    pub auth_api_key: String,
    pub bulk_location_callback_url: String,
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
}

impl AppState {
    pub async fn new(
        app_config: AppConfig,
        sender: Sender<(Dimensions, Latitude, Longitude, DriverId)>,
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
            non_persistent_redis,
            persistent_redis,
            drainer_delay: app_config.drainer_delay,
            drainer_size: app_config.drainer_size,
            new_ride_drainer_delay: app_config.new_ride_drainer_delay,
            sender,
            polygon: polygons,
            auth_url: app_config.auth_url,
            auth_api_key: app_config.auth_api_key,
            bulk_location_callback_url: app_config.bulk_location_callback_url,
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
        }
    }
}
