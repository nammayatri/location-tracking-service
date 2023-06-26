//! src/connection.rs

use dotenv::var;

pub fn connect() -> redis::Connection {
  let redis_host = var("REDIS_HOST").expect("missing environment variable REDIS_HOST");
  let redis_hostport = var("REDIS_HOSTPORT").expect("missing environment variable REDIS_HOSTPORT");

  let redis_password = var("REDIS_PASSWORD").unwrap_or_default();

  let redis_conn_url = format!("redis://{}@{}:{}", redis_password, redis_host, redis_hostport);

  println!("{}",redis_conn_url);

  redis::Client::open(redis_conn_url)
      .expect("Invalid connection URL")
      .get_connection()
      .expect("failed to connect to Redis")
}