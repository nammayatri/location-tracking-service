use crate::connection::connect;
use serde::Deserialize;
use std::env::var;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub redis_cfg: RedisConfig,
    pub auth_url: String,
    pub token_expiry: u64,
    pub location_expiry: u64,
    pub on_ride_expiry: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub redis_host: String,
    pub redis_port: u16,
}

pub fn read_dhall_config(config_path: &str) -> Result<AppConfig, String> {
    let config = serde_dhall::from_file(config_path).parse::<AppConfig>();
    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(format!("Error reading config: {}", e)),
    }
}

pub struct AppEnv {
    pub redis_conn: redis::Connection,
    pub auth_url: String,
    pub token_expiry: u64,
    pub location_expiry: u64,
    pub on_ride_expiry: u64,
}

pub fn make_app_env() -> AppEnv {
    let dhall_config_path =
        var("DHALL_CONFIG").unwrap_or_else(|_| "./dhall_configs/api_server.dhall".to_string());

    let app_config = read_dhall_config(&dhall_config_path).expect("Error reading config");

    let redis_conn: redis::Connection = connect(
        app_config.redis_cfg.redis_host,
        app_config.redis_cfg.redis_port,
    );
    AppEnv {
        redis_conn,
        auth_url: app_config.auth_url,
        token_expiry: app_config.token_expiry,
        location_expiry: app_config.location_expiry,
        on_ride_expiry: app_config.on_ride_expiry,
    }
}
