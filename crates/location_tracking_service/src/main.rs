use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use env_logger::Env;
use shared::utils::logger::*;
use shared::redis::interface::types::{RedisConnectionPool, RedisSettings};
use std::env::var;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

mod common;
use common::geo_polygon::read_geo_polygon;
use common::types::*;

mod domain;
use domain::api;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub location_redis_cfg: RedisConfig,
    pub generic_redis_cfg: RedisConfig,
    pub auth_url: String,
    pub token_expiry: u64,
    pub location_expiry: u64,
    pub on_ride_expiry: u64,
    pub test_location_expiry: usize,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub redis_host: String,
    pub redis_port: u16,
}

pub fn read_dhall_config(config_path: &str) -> Result<AppConfig, String> {
    let config = serde_dhall::from_file(config_path).parse::<AppConfig>();
    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(format!("Error reading config: {}", e)),
    }
}

pub async fn make_app_state(app_config: AppConfig) -> AppState {
    let location_redis = Arc::new(Mutex::new(
        RedisConnectionPool::new(&RedisSettings::new(app_config.location_redis_cfg.redis_host, app_config.location_redis_cfg.redis_port))
            .await
            .expect("Failed to create Critical Redis connection pool"),
    ));

    let generic_redis = Arc::new(Mutex::new(
        RedisConnectionPool::new(&RedisSettings::new(app_config.generic_redis_cfg.redis_host, app_config.generic_redis_cfg.redis_port))
            .await
            .expect("Failed to create Non Critical Redis connection pool"),
    ));

    let entries = Arc::new(Mutex::new(HashMap::new()));
    let polygons = read_geo_polygon("./config").expect("Failed to read geoJSON");

    AppState {
        location_redis,
        generic_redis,
        entries,
        polygon: polygons,
        auth_url: app_config.auth_url,
        token_expiry: app_config.token_expiry,
        location_expiry: app_config.location_expiry,
        on_ride_expiry: app_config.on_ride_expiry,
        test_location_expiry: app_config.test_location_expiry,
        location_update_limit: app_config.location_update_limit,
        location_update_interval: app_config.location_update_interval
    }
}

#[actix_web::main]
pub async fn start_server() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let dhall_config_path =
        var("DHALL_CONFIG").unwrap_or_else(|_| "./dhall_configs/api_server.dhall".to_string());
    let app_config = read_dhall_config(&dhall_config_path).unwrap();

    let app_state = make_app_state(app_config.clone()).await;

    let data = web::Data::new(app_state.clone());

    let location_expiry_in_sec = app_state.location_expiry;

    let test_loc_expiry_in_sec = app_state.test_location_expiry;

    let thread_data = data.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        let bucket =
            Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / location_expiry_in_sec;
        let mut entries = thread_data.entries.lock().unwrap();
        for merchant_id in entries.keys() {
            let entries = entries[merchant_id].to_owned();
            for city in entries.keys() {
                let entries = entries[city].to_owned();
                for vehicle_type in entries.keys() {
                    let entries = entries[vehicle_type].to_owned();

                    let key = format!("dl:loc:{merchant_id}:{city}:{vehicle_type}:{bucket}");

                    if !entries.is_empty() {
                        let redis = thread_data.location_redis.lock().unwrap();
                        let num = redis.zcard_sync(&key).expect("unable to zcard");
                        let _: () = redis
                            .geo_add_sync(&key, entries.to_vec())
                            .expect("Couldn't add to redis");
                        if num == 0 {
                            let _: () = redis
                                .set_expiry_sync(&key, test_loc_expiry_in_sec)
                                .expect("Unable to set expiry");
                        }
                        info!("Entries: {:?}\nSending to redis server", entries);
                        info!("^ Merchant id: {merchant_id}, City: {city}, Vt: {vehicle_type}, key: {key}\n");
                    }
                }
            }
        }
        entries.clear();
    });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(Logger::default())
            .configure(api::handler)
    })
    .bind(("127.0.0.1", app_config.port))?
    .run()
    .await
}

fn main() {
    start_server().expect("Failed to start the server");
}
