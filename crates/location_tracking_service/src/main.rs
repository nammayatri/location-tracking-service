use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use env_logger::Env;
use fred::types::{GeoPosition, GeoValue, MultipleGeoValues};
use rdkafka::error::KafkaError;
use shared::redis::interface::types::{RedisConnectionPool, RedisSettings};
use shared::utils::{
    logger::*,
    prometheus::{self, *},
};
use std::env::var;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::{spawn, sync::Mutex, time::Duration};

use std::{collections::HashMap, sync::Arc};

use location_tracking_service::common::{geo_polygon::read_geo_polygon, redis::*, types::*};
use location_tracking_service::domain::api;

use location_tracking_service::redis::commands::*;

use rdkafka::config::ClientConfig;
use rdkafka::producer::FutureProducer;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub location_redis_cfg: RedisConfig,
    pub generic_redis_cfg: RedisConfig,
    pub auth_url: String,
    pub auth_api_key: String,
    pub bulk_location_callback_url: String,
    pub token_expiry: u32,
    pub bucket_expiry: u64,
    pub on_ride_expiry: u32,
    pub min_location_accuracy: u32,
    pub redis_expiry: usize,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub kafka_cfg: KafkaConfig,
    pub driver_location_update_topic: String,
    pub driver_location_update_key: String,
    pub batch_size: u64,
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
}

pub fn read_dhall_config(config_path: &str) -> Result<AppConfig, String> {
    let config = serde_dhall::from_file(config_path).parse::<AppConfig>();
    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(format!("Error reading config: {}", e)),
    }
}

pub async fn make_app_state(app_config: AppConfig) -> AppState {
    let location_redis = Arc::new(
        RedisConnectionPool::new(&RedisSettings::new(
            app_config.location_redis_cfg.redis_host,
            app_config.location_redis_cfg.redis_port,
            app_config.location_redis_cfg.redis_pool_size,
        ))
        .await
        .expect("Failed to create Location Redis connection pool"),
    );

    let generic_redis = Arc::new(
        RedisConnectionPool::new(&RedisSettings::new(
            app_config.generic_redis_cfg.redis_host,
            app_config.generic_redis_cfg.redis_port,
            app_config.generic_redis_cfg.redis_pool_size,
        ))
        .await
        .expect("Failed to create Generic Redis connection pool"),
    );

    let queue = Arc::new(Mutex::new(HashMap::new()));
    let polygons = read_geo_polygon("./config").expect("Failed to read geoJSON");

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
        Err(e) => {
            producer = None;
            info!("Error connecting to kafka config: {}", e);
        }
    }

    AppState {
        location_redis,
        generic_redis,
        queue,
        polygon: polygons,
        auth_url: app_config.auth_url,
        auth_api_key: app_config.auth_api_key,
        bulk_location_callback_url: app_config.bulk_location_callback_url,
        token_expiry: app_config.token_expiry,
        bucket_expiry: app_config.bucket_expiry,
        min_location_accuracy: app_config.min_location_accuracy,
        on_ride_expiry: app_config.on_ride_expiry,
        redis_expiry: app_config.redis_expiry,
        location_update_limit: app_config.location_update_limit,
        location_update_interval: app_config.location_update_interval,
        producer,
        driver_location_update_topic: app_config.driver_location_update_topic,
        driver_location_update_key: app_config.driver_location_update_key,
        batch_size: app_config.batch_size,
    }
}

async fn run_drainer(data: web::Data<AppState>) {
    let bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / data.bucket_expiry;
    let mut queue = data.queue.lock().await;

    for (dimensions, queue) in queue.iter_mut() {
        let merchant_id = &dimensions.merchant_id;
        let city = &dimensions.city;
        let vehicle_type = &dimensions.vehicle_type;

        let geo_values: Vec<GeoValue> = queue
            .to_owned()
            .into_iter()
            .map(|(lon, lat, driver_id)| {
                prometheus::QUEUE_GUAGE.dec();
                GeoValue {
                    coordinates: GeoPosition {
                        longitude: lon,
                        latitude: lat,
                    },
                    member: driver_id.into(),
                }
            })
            .collect();
        let multiple_geo_values: MultipleGeoValues = geo_values.into();

        let key = driver_loc_bucket_key(merchant_id, city, &vehicle_type, &bucket);

        if !queue.is_empty() {
            let _ = push_drianer_values(data.clone(), key.clone(), multiple_geo_values).await;
            info!("queue: {:?}\nSending to redis server", queue);
            info!("^ Merchant id: {merchant_id}, City: {city}, Vt: {vehicle_type}, key: {key}\n");
        }
    }
    queue.clear();
}

#[actix_web::main]
async fn start_server() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let dhall_config_path =
        var("DHALL_CONFIG").unwrap_or_else(|_| "./dhall_configs/api_server.dhall".to_string());
    let app_config = read_dhall_config(&dhall_config_path).unwrap();

    let app_state = make_app_state(app_config.clone()).await;

    let data = web::Data::new(app_state.clone());

    let thread_data = data.clone();
    spawn(async move {
        loop {
            info!("scheduler executing...");
            let _ = tokio::time::sleep(Duration::from_secs(10)).await;
            run_drainer(thread_data.clone()).await
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(Logger::default())
            .wrap(prometheus_metrics())
            .configure(api::handler)
    })
    .bind(("127.0.0.1", app_config.port))?
    .run()
    .await
}

fn main() {
    start_server().expect("Failed to start the server");
}
