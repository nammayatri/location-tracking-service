use actix_web::dev::{Service, ServiceResponse};
use actix_web::{web, App, Error, HttpServer};
use futures::FutureExt;
use location_tracking_service::common::utils::get_current_bucket;
use rdkafka::error::KafkaError;
use shared::incoming_api;
use shared::redis::types::{RedisConnectionPool, RedisSettings};
use shared::tools::error::AppError;
use shared::utils::{logger::*, prometheus::*};
use std::env::var;
use std::time::Instant;
use tokio::{spawn, sync::Mutex, time::Duration};

use std::{collections::HashMap, sync::Arc};

use location_tracking_service::common::{geo_polygon::read_geo_polygon, types::*};
use location_tracking_service::domain::api;

use location_tracking_service::redis::commands::*;

use rdkafka::config::ClientConfig;
use rdkafka::producer::FutureProducer;
use serde::Deserialize;
use tracing_actix_web::TracingLogger;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub non_persistent_cfg: RedisConfig,
    pub drainer_delay: u64,
    pub persistent_redis_cfg: RedisConfig,
    pub auth_url: String,
    pub auth_api_key: String,
    pub bulk_location_callback_url: String,
    pub auth_token_expiry: u32,
    pub bucket_expiry: u64,
    pub redis_expiry: u32,
    pub min_location_accuracy: u32,
    pub last_location_timstamp_expiry: usize,
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
    let non_persistent_redis = Arc::new(
        RedisConnectionPool::new(&RedisSettings::new(
            app_config.non_persistent_cfg.redis_host,
            app_config.non_persistent_cfg.redis_port,
            app_config.non_persistent_cfg.redis_pool_size,
        ))
        .await
        .expect("Failed to create Location Redis connection pool"),
    );

    let persistent_redis = Arc::new(
        RedisConnectionPool::new(&RedisSettings::new(
            app_config.persistent_redis_cfg.redis_host,
            app_config.persistent_redis_cfg.redis_port,
            app_config.persistent_redis_cfg.redis_pool_size,
        ))
        .await
        .expect("Failed to create Generic Redis connection pool"),
    );

    let queue = Arc::new(Mutex::new(HashMap::new()));

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
        queue,
        polygon: polygons,
        auth_url: app_config.auth_url,
        auth_api_key: app_config.auth_api_key,
        bulk_location_callback_url: app_config.bulk_location_callback_url,
        auth_token_expiry: app_config.auth_token_expiry,
        bucket_expiry: app_config.bucket_expiry,
        min_location_accuracy: app_config.min_location_accuracy,
        redis_expiry: app_config.redis_expiry,
        last_location_timstamp_expiry: app_config.last_location_timstamp_expiry,
        location_update_limit: app_config.location_update_limit,
        location_update_interval: app_config.location_update_interval,
        producer,
        driver_location_update_topic: app_config.driver_location_update_topic,
        driver_location_update_key: app_config.driver_location_update_key,
        batch_size: app_config.batch_size,
    }
}

async fn run_drainer(data: web::Data<AppState>) -> Result<(), AppError> {
    let bucket = get_current_bucket(data.bucket_expiry)?;
    let mut queue = data.queue.lock().await;

    for (dimensions, geo_entries) in queue.iter() {
        let merchant_id = &dimensions.merchant_id;
        let city = &dimensions.city;
        let vehicle_type = &dimensions.vehicle_type;

        if !geo_entries.is_empty() {
            let _ = push_drainer_driver_location(
                data.clone(),
                merchant_id,
                city,
                vehicle_type,
                &bucket,
                geo_entries,
            )
            .await;
            info!(tag = "[Queued Entries For Draining]", length = %geo_entries.len(), "Queue: {:?}\nPushing to redis server", geo_entries);
        }
    }
    queue.clear();

    Ok(())
}

#[actix_web::main]
async fn start_server() -> std::io::Result<()> {
    setup_tracing();

    let dhall_config_path = var("DHALL_CONFIG")
        .unwrap_or_else(|_| "./dhall_config/location_tracking_service.dhall".to_string());
    let app_config = read_dhall_config(&dhall_config_path).unwrap_or_else(|err| {
        error!("Dhall Config Reading Error : {}", err);
        std::process::exit(1);
    });

    let app_state = make_app_state(app_config.clone()).await;

    let data = web::Data::new(app_state.clone());

    let thread_data = data.clone();
    spawn(async move {
        loop {
            info!(tag = "[Drainer]", "Draining From Queue to Redis...");
            let _ =
                tokio::time::sleep(Duration::from_secs(thread_data.clone().drainer_delay)).await;
            let _ = run_drainer(thread_data.clone()).await;
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap_fn(|req, srv| {
                let start_time = Instant::now();
                info!(tag = "[INCOMING REQUEST]", request_method = %req.method(), request_path = %req.path());
                srv.call(req)
                    .map(move |res: Result<ServiceResponse, Error>| {
                        let response = res?;
                        info!(tag = "[INCOMING API]", request_method = %response.request().method(), request_path = %response.request().path(), response_status = %response.status(), latency = format!("{:?}ms", start_time.elapsed().as_millis()));
                        incoming_api!(
                            response.request().method().as_str(),
                            response.request().uri().to_string().as_str(),
                            response.status().as_str(),
                            start_time
                        );
                        Ok(response)
                    })
            })
            .wrap(TracingLogger::default())
            .wrap(prometheus_metrics())
            .configure(api::handler)
    })
    .bind(("0.0.0.0", app_config.port))?
    .run()
    .await
}

fn main() {
    start_server().expect("Failed to start the server");
}
