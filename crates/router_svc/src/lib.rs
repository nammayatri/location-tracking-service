use actix_web::http::header::EntityTag;
use actix_web::middleware::Logger;
// use actix_web::web::Data;
use actix_web::{web, App, HttpServer};
use env_logger::Env;
use futures::executor::EnterError;
// use fred::types::{GeoPosition, GeoValue, MultipleGeoValues, RedisValue};
// use futures::executor;
// use futures::task::ArcWake;
use log::info;
use redis::Commands;
use redis_interface::{RedisConnectionPool, RedisSettings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env::var;
// use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
// use tokio::runtime::Runtime;
use tokio::time::Duration;

mod types;
use types::*;

mod tracking;
use tracking::services;

use actix::{Addr, SyncArbiter};
use diesel::{
    r2d2::{ConnectionManager, Pool},
    PgConnection,
};

pub const LIST_OF_VT: [&str; 4] = ["auto", "cab", "suv", "sedan"];
pub const LIST_OF_CITIES: [&str; 2] = ["blr", "ccu"];

// appstate for redis
pub struct AppState {
    pub redis_pool: Arc<Mutex<RedisConnectionPool>>,
    pub redis: Arc<Mutex<redis::Connection>>,
    pub entries: Arc<
        Mutex<
            HashMap<
                MerchantId,
                HashMap<CityName, HashMap<VehicleType, Vec<(Longitude, Latitude, DriverId)>>>,
            >,
        >,
    >,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Location {
    lat: Latitude,
    lon: Longitude,
    driver_id: DriverId,
    vehicle_type: VehicleType,
    merchant_id: MerchantId,
}

#[actix_web::main]
pub async fn start_server(conn: redis::Connection) -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    let data = web::Data::new(AppState {
        redis_pool: Arc::new(Mutex::new(
            RedisConnectionPool::new(&RedisSettings::default())
                .await
                .expect("Failed to create Redis connection pool"),
        )),
        redis: Arc::new(Mutex::new(conn)),
        entries: Arc::new(Mutex::new(HashMap::new())),
    });

    let location_expiry_in_sec = var("LOCATION_EXPIRY")
        .expect("LOCATION_EXPIRY not found")
        .parse::<usize>()
        .unwrap();

    let thread_data = data.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        // Access the vector in the separate thread's lifetime
        // println!("started thread");
        let bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / 60;
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
                                .expire(&key, location_expiry_in_sec)
                                .expect("Unable to set expiry");
                        }
                        info!("Entries: {:?}\nSending to redis server", entries);
                        info!("^ Merchant id: {merchant_id}, City: {city}, Vt: {vehicle_type}, key: {key}\n");
                    }
                }
            }
        }
        entries.clear();
        //     for vt in LIST_OF_VT {
        //         // println!("hello?");

        //         let mut entries = thread_data.entries.lock().unwrap();

        //         // println!("entries: {:?}", entries);
        //         // entries.sort_by(|a, b| a.3.cmp(&b.3));
        //         // println!("sorted entries: {:?}", entries);

        //         for city in LIST_OF_CITIES {
        //             //println!("entries: {:?}", entries);
        //             let new_entries = entries[vt]
        //                 .clone()
        //                 .into_iter()
        //                 .filter(|x| x.3 == city)
        //                 .map(|x| (x.0, x.1, x.2))
        //                 .collect::<Vec<(f64, f64, String)>>();

        //             let key = format!("dl:loc:{}:{}:{}", city, vt, bucket);
        //             // println!("key: {}", key);

        //             if !new_entries.is_empty() {
        //                 let mut redis = thread_data.redis.lock().unwrap();
        //                 let num = redis.zcard::<_, u64>(&key).expect("Unable to zcard");
        //                 let _: () = redis
        //                     .geo_add(&key, new_entries.to_vec())
        //                     .expect("Couldn't add to redis");
        //                 if num == 0 {
        //                     let _: () = redis
        //                         .expire(&key, location_expiry_in_sec)
        //                         .expect("Unable to set expiry time");
        //                     // info!("num 0");
        //                 }

        //                 info!("Entries: {:?}\nSending to redis server", new_entries);
        //                 info!("^  Vt: {}, key: {}\n", vt, key);
        //             }
        //         }
        //         entries.clear()
        //     }
    });

    // let thread_data = data.clone();
    // thread::spawn(move || {
    //     let mut key = format!(
    //         "dl:loc:blr:auto:{}",
    //         Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap())
    //     );
    //     loop {
    //         if let Ok(mut current_bucket) = thread_data.current_bucket.lock() {
    //             *current_bucket =
    //                 Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / 60;
    //             key = format!("dl:loc:blr:auto:{}", current_bucket);
    //             info!("Current key: {}", key);
    //         }
    //         if let mut redis = thread_data.redis.lock().unwrap() {
    //             let _: () = redis
    //                 .geo_add(&key, (0.1, 0.1, "foo"))
    //                 .expect("Couldn't add default value to key");
    //             let _: () = redis.expire(&key, 90).expect("Couldn't set expiry time");
    //         }
    //         thread::sleep(Duration::from_secs(60));
    //     }
    // });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(Logger::default())
            .configure(services::config)
    })
    .bind(("127.0.0.1", 8081))?
    .run()
    .await
}
