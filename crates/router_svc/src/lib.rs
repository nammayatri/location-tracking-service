use actix_web::middleware::Logger;
use actix_web::web::Data;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use env_logger::Env;
use fred::types::{GeoPosition, GeoValue, MultipleGeoValues, RedisValue};
use futures::executor;
use futures::task::ArcWake;
use log::info;
use redis::Commands;
use redis_interface::{RedisConnectionPool, RedisSettings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env::var;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use tokio::time::Duration;

mod tracking;
use tracking::services;

pub const LIST_OF_VT: [&str; 4] = ["auto", "cab", "suv", "sedan"];

// appstate for redis
pub struct AppState {
    pub redis_pool: Arc<Mutex<RedisConnectionPool>>,
    pub redis: Arc<Mutex<redis::Connection>>,
    pub entries: HashMap<String, Arc<Mutex<Vec<(f64, f64, String, String)>>>>,
    pub current_bucket: Mutex<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Location {
    lat: f64,
    lon: f64,
    driver_id: String,
    vt: String,
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
        entries: HashMap::from(
            LIST_OF_VT.map(|x| (x.to_string(), Arc::new(Mutex::new(Vec::new())))),
        ),
        current_bucket: Mutex::new(Duration::as_secs(
            &SystemTime::elapsed(&UNIX_EPOCH).unwrap(),
        )),
    });

    let location_expiry_in_sec = var("LOCATION_EXPIRY")
        .expect("LOCATION_EXPIRY not found")
        .parse::<usize>()
        .unwrap();

    let thread_data = data.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        // Access the vector in the separate thread's lifetime
        for item in LIST_OF_VT {
            let bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / 60;
            let key = format!("dl:loc:blr:{}:{}", item, bucket);
            let mut entries = thread_data.entries[item].lock().unwrap();
            let new_entries = entries
                .clone()
                .into_iter()
                .map(|x| (x.0, x.1, x.2))
                .collect::<Vec<(f64, f64, String)>>();
            if !entries.is_empty() {
                if let mut redis = thread_data.redis.lock().unwrap() {
                    let num = redis.zcard::<_, u64>(&key).expect("Unable to zcard");
                    let _: () = redis
                        .geo_add(&key, new_entries.to_vec())
                        .expect("Couldn't add to redis");
                    if num == 0 {
                        let _: () = redis
                            .expire(&key, location_expiry_in_sec)
                            .expect("Unable to set expiry time");
                        info!("num 0");
                    }
                }
                info!("Entries: {:?}\nSending to redis server", entries);
                info!("^  Vt: {}, key: {}", item, key);
                entries.clear();
            } else {
                info!("Bucket: {}", bucket);
            }
        }
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
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
