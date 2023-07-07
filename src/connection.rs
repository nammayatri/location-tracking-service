//! src/connection.rs

use std::env;

pub fn connect() -> redis::Connection {
    let redis_host = env::var("REDIS_HOST").expect("REDIS_HOST not found");
    // let redis_port = env::var("REDIS_PORT").expect("REDIS_PORT not found");

    //let redis_password = var("REDIS_PASSWORD").unwrap_or_default();

    let redis_conn_url = format!("redis://{}", redis_host);

    println!("{}", redis_conn_url);

    redis::Client::open(redis_conn_url)
        .expect("Invalid connection URL")
        .get_connection()
        .expect("failed to connect to Redis")
}
