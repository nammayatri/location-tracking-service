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
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use tokio::time::Duration;

mod tracking;
use tracking::services;

// appstate for redis
pub struct AppState {
    pub redis_pool: Arc<Mutex<RedisConnectionPool>>,
    pub redis: Arc<Mutex<redis::Connection>>,
    pub entries: Arc<Mutex<Vec<(f64, f64, String)>>>,
    pub current_bucket: Arc<Mutex<u64>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Location {
    lat: f64,
    lon: f64,
    driver_id: String,
}

// async fn bucket_change(thread_data: Data<AppState>) {
//     let mut current_bucket = thread_data.current_bucket.lock().unwrap();
//     *current_bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap());
//     for item in ["auto", "cab"] {
//         let key = format!("dl:loc:blr:{}:{}", item, current_bucket);
//         info!("Current key: {}", key);
//         let mut redis_pool = thread_data.redis_pool.lock().unwrap();
//         let _ = redis_pool
//             .geo_add(
//                 &key,
//                 vec![GeoValue {
//                     coordinates: GeoPosition {
//                         longitude: 0.0,
//                         latitude: 0.0,
//                     },
//                     member: RedisValue::String("foo".into()),
//                 }],
//                 None,
//                 false,
//             )
//             .await;
//         let mut redis_pool = thread_data.redis_pool.lock().unwrap();
//         let _ = redis_pool.set_expiry(&key, 90).await;
//     }
//     thread::sleep(Duration::from_secs(60));
// }

// async fn redis_flush(thread_data: Data<AppState>) {
//     thread::sleep(Duration::from_secs(10));
//     // Access the vector in the separate thread's lifetime
//     if let mut redis_pool = thread_data.redis_pool.lock().unwrap() {
//         let mut entries = thread_data.entries.lock().unwrap();
//         let _ = redis_pool
//             .geo_add("drivers", (*entries).clone(), None, false)
//             .await;
//         let mut entries = thread_data.entries.lock().unwrap();
//         info!("Entries: {:?}\nSending to redis server", entries);
//         entries.clear();
//     }
// }

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
        entries: Arc::new(Mutex::new(Vec::new())),
        current_bucket: Arc::new(Mutex::new(Duration::as_secs(
            &SystemTime::elapsed(&UNIX_EPOCH).unwrap(),
        ))),
    });

    let thread_data = data.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        // Access the vector in the separate thread's lifetime
        let key = format!(
            "dl:loc:blr:auto:{}",
            thread_data.current_bucket.lock().unwrap()
        );
        let mut entries = thread_data.entries.lock().unwrap();
        if !entries.is_empty() {
            if let mut redis = thread_data.redis.lock().unwrap() {
                let _: () = redis
                    .geo_add(key, entries.to_vec())
                    .expect("Couldn't add to redis");
            }
            info!("Entries: {:?}\nSending to redis server", entries);
            entries.clear();
        } else {
            info!("Entries: {:?}", entries);
        }
    });

    let thread_data = data.clone();
    thread::spawn(move || loop {
        let mut key: String = String::new();
        if let Ok(mut current_bucket) = thread_data.current_bucket.lock() {
            *current_bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap());
            key = format!("dl:loc:blr:auto:{}", current_bucket);
            info!("Current key: {}", key);
        }
        if let mut redis = thread_data.redis.lock().unwrap() {
            let _: () = redis
                .geo_add(&key, (0.1, 0.1, "foo"))
                .expect("Couldn't add default value to key");
            let _: () = redis.expire(&key, 90).expect("Couldn't set expiry time");
        }
        thread::sleep(Duration::from_secs(60));
    });

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
