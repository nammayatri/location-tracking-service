//! src/connection.rs

use std::env::var;

pub fn connect() -> redis::Connection {
    let redis_host = var("REDIS_HOST").expect("missing environment variable REDIS_HOST");

    let redis_conn_url = format!(
        "redis://{}",
        redis_host
    );

    println!("{}", redis_conn_url);

    redis::Client::open(redis_conn_url)
        .expect("Invalid connection URL")
        .get_connection()
        .expect("failed to connect to Redis")
}
