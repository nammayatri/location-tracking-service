use chrono::{DateTime, Utc};
use geo::MultiPolygon;
use serde::{Deserialize, Serialize};
use shared::redis::interface::types::RedisConnectionPool;
use shared::utils::logger::*;
use tokio::sync::Mutex;
use std::{collections::HashMap, sync::Arc, time::{UNIX_EPOCH, SystemTime}};
use strum_macros::{EnumString, Display};
use super::errors::AppError;

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum VehicleType {
    #[strum(serialize = "AUTO_RICKSHAW")]
    #[serde(rename = "AUTO_RICKSHAW")]
    AutoRickshaw,
    #[strum(serialize = "SEDAN")]
    #[serde(rename = "SEDAN")]
    Sedan,
    #[strum(serialize = "SUV")]
    #[serde(rename = "SUV")]
    SUV,
    #[strum(serialize = "HATCHBACK")]
    #[serde(rename = "HATCHBACK")]
    Hatchback,
}

pub type DriverId = String;
pub type MerchantId = String;
pub type Latitude = f64;
pub type Longitude = f64;
pub type CityName = String;
pub type TimeStamp = DateTime<Utc>;
pub type Radius = f64;
pub type Accuracy = i32;
pub type Key = String;
pub type Token = String;

#[derive(Debug, Serialize)]
pub struct APISuccess {
    result : String
}

impl Default for APISuccess {
    fn default() -> Self {
        Self { result: "success".to_string() }
    }
}

#[derive(Clone)]
pub struct MultiPolygonBody {
    pub region: String,
    pub multipolygon: MultiPolygon,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RideId {
    pub on_ride: bool,
    pub ride_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Point {
    pub lat: Latitude,
    pub lon: Longitude,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Dimensions {
    pub merchant_id: MerchantId,
    pub city: CityName,
    pub vehicle_type: VehicleType,
}

#[derive(Clone)]
pub struct AppState {
    pub location_redis: Arc<Mutex<RedisConnectionPool>>,
    pub generic_redis: Arc<Mutex<RedisConnectionPool>>,
    pub entries: Arc<
        Mutex<
            HashMap<Dimensions, Vec<(Longitude, Latitude, DriverId)>>
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
}

impl AppState {
    pub async fn sliding_window_limiter(
        &self,
        key: &str,
        frame_hits_lim: usize,
        frame_len: u32,
        redis_pool: &tokio::sync::MutexGuard<'_, RedisConnectionPool>,
    ) -> Result<Vec<i64>, AppError> {
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

        if !ret {
            return Err(AppError::HitsLimitExceeded);
        }

        let _ = redis_pool.set_with_expiry(key, serde_json::to_string(&filt_hits).unwrap(), frame_len).await;

        Ok(filt_hits)
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

pub fn on_ride_key(merchant_id: &String, city: &String, driver_id: &String) -> String {
    format!("ds:on_ride:{merchant_id}:{city}:{driver_id}")
}

pub fn on_ride_loc_key(merchant_id: &String, city: &String, driver_id: &String) -> String {
    format!("dl:loc:{merchant_id}:{city}:{driver_id}")
}

pub fn driver_loc_ts_key(driver_id: &String) -> String {
    format!("dl:ts:{}", driver_id)
}

pub fn driver_loc_bucket_key(
    merchant_id: &String,
    city: &String,
    vehicle_type: &String,
    bucket: &u64,
) -> String {
    format!("dl:loc:{merchant_id}:{city}:{vehicle_type}:{bucket}")
}