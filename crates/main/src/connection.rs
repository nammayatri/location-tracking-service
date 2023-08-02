//! src/connection.rs

pub fn connect(redis_host: String, redis_port: u16) -> redis::Connection {
    let redis_conn_url = format!("redis://{}:{}", redis_host, redis_port);

    println!("Connecting to Redis {}", redis_conn_url);

    redis::Client::open(redis_conn_url)
        .expect("Invalid connection URL")
        .get_connection()
        .expect("failed to connect to Redis")
}
