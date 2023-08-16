use crate::tracking;
use crate::Arc;
use crate::CityName;
use crate::DriverId;
use crate::HashMap;
use crate::Latitude;
use crate::Longitude;
use crate::MerchantId;
use crate::MultiPolygonBody;
use crate::Mutex;
use crate::RedisConnectionPool;
use crate::RedisSettings;
use crate::VehicleType;
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub redis_cfg: RedisConfig,
    pub auth_url: String,
    pub token_expiry: u64,
    pub location_expiry: u64,
    pub on_ride_expiry: u64,
    pub test_location_expiry: usize,
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

#[derive(Clone)]
pub struct AppState {
    pub redis_pool: Arc<Mutex<RedisConnectionPool>>,
    pub redis: Arc<Mutex<redis::Connection>>,
    pub entries: Arc<
        Mutex<
            HashMap<
                MerchantId,
                HashMap<CityName, HashMap<VehicleType, Vec<(Longitude, Latitude, DriverId)>>>,
            >,
        >,
    >,
    pub polygon: Vec<MultiPolygonBody>,
    pub auth_url: String,
    pub token_expiry: u64,
    pub location_expiry: u64,
    pub on_ride_expiry: u64,
    pub test_location_expiry: usize,
    pub user_limits: Arc<Mutex<HashMap<String, (usize, Instant)>>>,
}

impl AppState {
    pub async fn check_rate_limit(&self, user_id: &str, limit: usize, interval: Duration) -> bool {
        let mut user_limits = self.user_limits.lock().expect("Failed to get user limits");
        let (count, last_request_time) = user_limits
            .entry(user_id.to_string())
            .or_insert((0, Instant::now()));
        let elapsed = last_request_time.elapsed();

        if elapsed >= interval {
            *count = 1;
            *last_request_time = Instant::now();
            true
        } else if *count < limit {
            *count += 1;
            true
        } else {
            false
        }
    }
}

pub async fn make_app_state(app_config: AppConfig) -> AppState {
    // Connect to Redis
    let redis_conn_url = format!(
        "redis://{}:{}",
        app_config.redis_cfg.redis_host, app_config.redis_cfg.redis_port
    );

    println!("Connecting to Redis {}", redis_conn_url);

    let redis_conn: redis::Connection = redis::Client::open(redis_conn_url)
        .expect("Invalid connection URL")
        .get_connection()
        .expect("failed to connect to Redis");

    let redis_pool = Arc::new(Mutex::new(
        RedisConnectionPool::new(&RedisSettings::default())
            .await
            .expect("Failed to create Redis connection pool"),
    ));

    let redis = Arc::new(Mutex::new(redis_conn));

    // Create a hashmap to store the entries
    let entries = Arc::new(Mutex::new(HashMap::new()));

    // Read the geo polygons
    let polygons =
        tracking::geo_polygon::read_geo_polygon("./config").expect("Failed to read geoJSON");

    let user_limits = Arc::new(Mutex::new(HashMap::new()));

    AppState {
        redis_pool,
        redis,
        entries,
        polygon: polygons,
        auth_url: app_config.auth_url,
        token_expiry: app_config.token_expiry,
        location_expiry: app_config.location_expiry,
        on_ride_expiry: app_config.on_ride_expiry,
        test_location_expiry: app_config.test_location_expiry,
        user_limits,
    }
}
