use actix_web::middleware::Logger;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use env_logger::Env;
use redis_interface::{RedisConnectionPool, RedisSettings};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

mod tracking;
use tracking::services;

// appstate for redis
pub struct AppState {
    pub redis_pool: Arc<Mutex<RedisConnectionPool>>,
}

#[actix_web::main]
pub async fn start_server() -> std::io::Result<()> {
    let data = web::Data::new(AppState {
        redis_pool: Arc::new(Mutex::new(
            RedisConnectionPool::new(&RedisSettings::default())
                .await
                .expect("Failed to create Redis connection pool"),
        )),
    });
    env_logger::init_from_env(Env::default().default_filter_or("info"));
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
