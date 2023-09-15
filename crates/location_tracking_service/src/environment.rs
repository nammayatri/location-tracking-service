use std::{
    env::var,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use rdkafka::{error::KafkaError, producer::FutureProducer, ClientConfig};
use serde::Deserialize;
use shared::{
    redis::types::{RedisConnectionPool, RedisSettings},
    tools::error::AppError,
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
    pub drainer_delay: u64,
    pub drainer_size: usize,
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
    pub driver_location_update_key: String,
    pub batch_size: i64,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
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
    pub driver_location_update_key: String,
    pub batch_size: i64,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
}

impl AppState {
    pub async fn new(
        app_config: AppConfig,
        sender: Sender<(Dimensions, Latitude, Longitude, DriverId)>,
    ) -> AppState {
        let non_persistent_redis_settings = if app_config.redis_migration_stage {
            RedisSettings::new(
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
            )
        } else {
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
            )
        };

        let non_persistent_redis = Arc::new(
            RedisConnectionPool::new(&non_persistent_redis_settings)
                .await
                .expect("Failed to create Location Redis connection pool"),
        );

        let persistent_redis_settings = if app_config.redis_migration_stage {
            RedisSettings::new(
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
            )
        } else {
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
            )
        };

        let persistent_redis = Arc::new(
            RedisConnectionPool::new(&persistent_redis_settings)
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
            driver_location_update_key: app_config.driver_location_update_key,
            batch_size: app_config.batch_size,
            bucket_size: app_config.bucket_size,
            nearby_bucket_threshold: app_config.nearby_bucket_threshold,
        }
    }

    pub async fn sliding_window_limiter(
        &self,
        key: &str,
        frame_hits_lim: usize,
        frame_len: u32,
        persistent_redis_pool: &RedisConnectionPool,
    ) -> Result<Vec<i64>, AppError> {
        let curr_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs() as i64;

        let hits = persistent_redis_pool.get_key(key).await?;

        let hits = match hits {
            Some(hits) => serde_json::from_str::<Vec<i64>>(&hits).map_err(|_| {
                AppError::InternalError("Failed to parse hits from redis.".to_string())
            })?,
            None => vec![],
        };
        let (filt_hits, ret) =
            Self::sliding_window_limiter_pure(curr_time, &hits, frame_hits_lim, frame_len);

        if !ret {
            return Err(AppError::HitsLimitExceeded);
        }

        let _ = persistent_redis_pool
            .set_with_expiry(
                key,
                serde_json::to_string(&filt_hits).expect("Failed to parse filt_hits to string."),
                frame_len,
            )
            .await;

        Ok(filt_hits)
    }

    fn sliding_window_limiter_pure(
        curr_time: i64,
        hits: &[i64],
        frame_hits_lim: usize,
        frame_len: u32,
    ) -> (Vec<i64>, bool) {
        let curr_frame = Self::get_time_frame(curr_time, frame_len);
        let filt_hits = hits
            .iter()
            .filter(|&&hit| Self::hits_filter(curr_frame, hit))
            .cloned()
            .collect::<Vec<_>>();
        let prev_frame_hits_len = filt_hits
            .iter()
            .filter(|&&hit| Self::prev_frame_hits_filter(curr_frame, hit))
            .count();
        let prev_frame_weight = 1.0 - (curr_time as f64 % frame_len as f64) / frame_len as f64;
        let curr_frame_hits_len: i32 = filt_hits
            .iter()
            .filter(|&&hit| Self::curr_frame_hits_filter(curr_frame, hit))
            .count() as i32;

        let res = (prev_frame_hits_len as f64 * prev_frame_weight) as i32 + curr_frame_hits_len
            < frame_hits_lim as i32;

        (
            if res {
                let mut new_hits = Vec::with_capacity(filt_hits.len() + 1);
                new_hits.push(curr_frame);
                new_hits.extend(filt_hits);
                new_hits
            } else {
                filt_hits.clone()
            },
            res,
        )
    }

    fn get_time_frame(time: i64, frame_len: u32) -> i64 {
        time / frame_len as i64
    }

    fn hits_filter(curr_frame: i64, time_frame: i64) -> bool {
        time_frame == curr_frame - 1 || time_frame == curr_frame
    }

    fn prev_frame_hits_filter(curr_frame: i64, time_frame: i64) -> bool {
        time_frame == curr_frame - 1
    }

    fn curr_frame_hits_filter(curr_frame: i64, time_frame: i64) -> bool {
        time_frame == curr_frame
    }
}
