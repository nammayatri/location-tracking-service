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
use fred::tracing::info;
use rdkafka::config::ClientConfig;
use rdkafka::producer::FutureProducer;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub redis_cfg: RedisConfig,
    pub kafka_cfg: KafkaConfig,
    pub auth_url: String,
    pub token_expiry: u64,
    pub location_expiry: u64,
    pub on_ride_expiry: u64,
    pub test_location_expiry: usize,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub driver_location_update_topic: String,
    pub driver_location_update_key: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub redis_host: String,
    pub redis_port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub kafka_key: String,
    pub kafka_host: String,
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
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub producer: FutureProducer,
    pub driver_location_update_topic: String,
    pub driver_location_update_key: String,
}

impl AppState {
    pub async fn sliding_window_limiter(
        &self,
        key: &str,
        frame_hits_lim: usize,
        frame_len: u32,
        redis_pool: &std::sync::MutexGuard<'_, redis_interface::RedisConnectionPool>,
    ) -> (Vec<i64>, bool) {
        let curr_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs() as i64;

        let hits = redis_pool.get_key::<String>(key).await.unwrap();
        let nil_string = String::from("nil");
        let hits = if hits == nil_string {
            vec![]
        } else {
            serde_json::from_str::<Vec<i64>>(&hits).unwrap()
        };

        info!("hits: {:?}", hits);

        let (filt_hits, ret) =
            Self::sliding_window_limiter_pure(curr_time, &hits, frame_hits_lim, frame_len);

        if ret {
            let filt_hits = serde_json::to_string(&filt_hits).unwrap();
            let _ = redis_pool.set_with_expiry(key, filt_hits, frame_len).await;
        }

        drop(redis_pool);

        (filt_hits, ret)
    }

    fn sliding_window_limiter_pure(
        curr_time: i64,
        hits: &[i64],
        frame_hits_lim: usize,
        frame_len: u32,
    ) -> (Vec<i64>, bool) {
        let curr_frame = Self::get_time_frame(curr_time, frame_len);
        let filt_hits = hits
            .iter()
            .filter(|&&hit| Self::hits_filter(curr_frame, hit))
            .cloned()
            .collect::<Vec<_>>();
        let prev_frame_hits_len = filt_hits
            .iter()
            .filter(|&&hit| Self::prev_frame_hits_filter(curr_frame, hit))
            .count();
        let prev_frame_weight = 1.0 - (curr_time as f64 % frame_len as f64) / frame_len as f64;
        let curr_frame_hits_len: i32 = filt_hits
            .iter()
            .filter(|&&hit| Self::curr_frame_hits_filter(curr_frame, hit))
            .count() as i32;

        let res = (prev_frame_hits_len as f64 * prev_frame_weight) as i32 + curr_frame_hits_len
            < frame_hits_lim as i32;

        (
            if res {
                let mut new_hits = Vec::with_capacity(filt_hits.len() + 1);
                new_hits.push(curr_frame);
                new_hits.extend(filt_hits);
                new_hits
            } else {
                filt_hits.clone()
            },
            res,
        )
    }

    fn get_time_frame(time: i64, frame_len: u32) -> i64 {
        time / frame_len as i64
    }

    fn hits_filter(curr_frame: i64, time_frame: i64) -> bool {
        time_frame == curr_frame - 1 || time_frame == curr_frame
    }

    fn prev_frame_hits_filter(curr_frame: i64, time_frame: i64) -> bool {
        time_frame == curr_frame - 1
    }

    fn curr_frame_hits_filter(curr_frame: i64, time_frame: i64) -> bool {
        time_frame == curr_frame
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

    let producer: FutureProducer = ClientConfig::new()
        .set(
            app_config.kafka_cfg.kafka_key,
            app_config.kafka_cfg.kafka_host,
        )
        .set("compression.type", "lz4")
        .create()
        .expect("Producer creation error");

    let redis = Arc::new(Mutex::new(redis_conn));

    // Create a hashmap to store the entries
    let entries = Arc::new(Mutex::new(HashMap::new()));

    // Read the geo polygons
    let polygons =
        tracking::geo_polygon::read_geo_polygon("./config").expect("Failed to read geoJSON");

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
        location_update_limit: app_config.location_update_limit,
        location_update_interval: app_config.location_update_interval,
        producer,
        driver_location_update_topic: app_config.driver_location_update_topic,
        driver_location_update_key: app_config.driver_location_update_key,
    }
}

//Redis keys

pub async fn on_ride_key(merchant_id: &String, city: &String, driver_id: &String) -> String {
    format!("ds:on_ride:{merchant_id}:{city}:{driver_id}")
}

pub async fn on_ride_loc_key(merchant_id: &String, city: &String, driver_id: &String) -> String {
    format!("dl:loc:{merchant_id}:{city}:{driver_id}")
}

pub async fn driver_loc_ts_key(driver_id: &String) -> String {
    format!("dl:ts:{}", driver_id)
}

pub async fn driver_loc_bucket_key(
    merchant_id: &String,
    city: &String,
    vehicle_type: &String,
    bucket: &u64,
) -> String {
    format!("dl:loc:{merchant_id}:{city}:{vehicle_type}:{bucket}")
}

pub async fn driver_loc_bucket_keys_with_all_vt(
    merchant_id: &String,
    city: &String,
    bucket: &u64,
) -> String {
    format!("dl:loc:{merchant_id}:{city}:*:{bucket}")
}
