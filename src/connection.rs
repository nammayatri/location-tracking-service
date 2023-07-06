//! src/connection.rs

//use dotenv::var;

pub fn connect() -> redis::Connection {
    let redis_host = "127.0.0.1";
    let redis_port = 6379;

    //let redis_password = var("REDIS_PASSWORD").unwrap_or_default();

    let redis_conn_url = format!("redis://{}:{}", redis_host, redis_port);

    println!("{}", redis_conn_url);

    redis::Client::open(redis_conn_url)
        .expect("Invalid connection URL")
        .get_connection()
        .expect("failed to connect to Redis")
}
