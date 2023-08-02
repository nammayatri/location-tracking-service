use crate::env::AppState;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};
use env_logger::Env;
use log::info;
use redis::Commands;
use redis_interface::{RedisConnectionPool, RedisSettings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env::var;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;

mod types;
use types::*;

pub mod tracking;
use tracking::models::MultiPolygonBody;
use tracking::services;

pub mod env;
use env::make_app_state;

pub const LIST_OF_VT: [&str; 4] = ["auto", "cab", "suv", "sedan"];
pub const LIST_OF_CITIES: [&str; 2] = ["blr", "ccu"];

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Location {
    lat: Latitude,
    lon: Longitude,
    driver_id: DriverId,
    vehicle_type: VehicleType,
    merchant_id: MerchantId,
}

#[actix_web::main]
pub async fn start_server() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let dhall_config_path =
        var("DHALL_CONFIG").unwrap_or_else(|_| "./dhall_configs/api_server.dhall".to_string());
    let app_config = env::read_dhall_config(&dhall_config_path).unwrap();

    let app_state = make_app_state(app_config.clone()).await;

    let data = web::Data::new(app_state.clone());

    let location_expiry_in_sec = app_state.location_expiry;

    let test_loc_expiry_in_sec = app_state.test_location_expiry;

    let thread_data = data.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        // Access the vector in the separate thread's lifetime
        // println!("started thread");
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
                        let mut redis = thread_data.redis.lock().unwrap();
                        let num = redis.zcard::<_, u64>(&key).expect("unable to zcard");
                        let _: () = redis
                            .geo_add(&key, entries.to_vec())
                            .expect("Couldn't add to redis");
                        if num == 0 {
                            let _: () = redis
                                .expire(&key, test_loc_expiry_in_sec)
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
            .configure(services::config)
    })
    .bind(("127.0.0.1", app_config.port))?
    .run()
    .await
}
