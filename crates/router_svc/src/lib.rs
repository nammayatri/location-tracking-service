use actix_web::middleware::Logger;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use env_logger::Env;
use log::info;
use redis_interface::{RedisConnectionPool, RedisSettings};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::time::Duration;

mod tracking;
use tracking::services;

use actix::{Addr, SyncArbiter};
use diesel::{
    r2d2::{ConnectionManager, Pool},
    PgConnection,
};
pub mod db_models;
use std::env;
mod db_utils;
use db_utils::{get_pool, DbActor};

// appstate for redis
pub struct AppState {
    pub redis_pool: Arc<Mutex<RedisConnectionPool>>,
    pub entries: Arc<Mutex<Vec<(f64, f64, String)>>>,
    pub db: Addr<DbActor>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Location {
    lat: f64,
    lon: f64,
    driver_id: String,
}

#[actix_web::main]
pub async fn start_server() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = get_pool(&db_url);
    // let pool: ConnectionManager::<PgConnection> = ConnectionManager::<PgConnection>::new(db_url);
    // Pool::builder().build(pool).expect("Error building a connection pool.");
    let db_addr = SyncArbiter::start(5, move || DbActor(pool.clone()));
    let mut data = web::Data::new(AppState {
        redis_pool: Arc::new(Mutex::new(
            RedisConnectionPool::new(&RedisSettings::default())
                .await
                .expect("Failed to create Redis connection pool"),
        )),
        entries: Arc::new(Mutex::new(vec![])),
        db: db_addr.clone(),
    });

    // Spawn a thread to run a separate process
    let thread_data = data.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(10));
            // Access the vector in the separate thread's lifetime
            if let mut entries = thread_data.entries.lock().unwrap() {
                if let mut redis = thread_data.redis_pool.lock().unwrap() {
                    info!("Entries: {:?}\nSending to redis server", entries);
                    entries.clear();
                }
            }
            // for item in entries.iter() {
            //     info!("xyz {:?}", item);
            // }
        }
    });

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
