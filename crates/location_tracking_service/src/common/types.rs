use chrono::{DateTime, Utc};
use geo::MultiPolygon;
use rdkafka::producer::FutureProducer;
use serde::{Deserialize, Serialize};
use shared::redis::types::RedisConnectionPool;
use shared::tools::error::AppError;
use shared::utils::logger::*;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use strum_macros::{Display, EnumIter, EnumString};
use tokio::sync::Mutex;

pub type DriverId = String;
pub type MerchantId = String;
pub type Latitude = f64;
pub type Longitude = f64;
pub type CityName = String;
pub type TimeStamp = DateTime<Utc>;
pub type Radius = f64;
pub type Accuracy = i32;
pub type Token = String;

#[derive(
    Debug, Clone, EnumString, EnumIter, Display, Serialize, Deserialize, Eq, Hash, PartialEq,
)]
pub enum VehicleType {
    #[strum(serialize = "AUTO_RICKSHAW")]
    #[serde(rename = "AUTO_RICKSHAW")]
    AutoRickshaw,
    #[strum(serialize = "SEDAN")]
    #[serde(rename = "SEDAN")]
    Sedan,
    SUV,
    #[strum(serialize = "HATCHBACK")]
    #[serde(rename = "HATCHBACK")]
    Hatchback,
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum RideStatus {
    NEW,
    INPROGRESS,
    COMPLETED,
    CANCELLED,
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum DriverMode {
    ONLINE,
    OFFLINE,
    SILENT,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct APISuccess {
    result: String,
}

impl Default for APISuccess {
    fn default() -> Self {
        Self {
            result: "Success".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthData {
    #[serde(rename = "driverId")]
    pub driver_id: String,
}

pub struct DriverLocationPoint {
    pub driver_id: String,
    pub location: Point,
}

#[derive(Clone)]
pub struct MultiPolygonBody {
    pub region: String,
    pub multipolygon: MultiPolygon,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct RideDetails {
    pub ride_id: String,
    pub ride_status: RideStatus,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct DriverDetails {
    pub driver_id: DriverId,
    pub driver_mode: DriverMode,
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
    pub location_redis: Arc<RedisConnectionPool>,
    pub generic_redis: Arc<RedisConnectionPool>,
    pub queue: Arc<Mutex<HashMap<Dimensions, Vec<(Latitude, Longitude, DriverId)>>>>,
    pub polygon: Vec<MultiPolygonBody>,
    pub auth_url: String,
    pub auth_api_key: String,
    pub bulk_location_callback_url: String,
    pub token_expiry: u32,
    pub bucket_expiry: u64,
    pub on_ride_expiry: u32,
    pub min_location_accuracy: u32,
    pub redis_expiry: usize,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub producer: Option<FutureProducer>,
    pub driver_location_update_topic: String,
    pub driver_location_update_key: String,
    pub batch_size: u64,
}

impl AppState {
    pub async fn sliding_window_limiter(
        &self,
        key: &str,
        frame_hits_lim: usize,
        frame_len: u32,
        generic_redis_pool: &RedisConnectionPool,
    ) -> Result<Vec<i64>, AppError> {
        let curr_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs() as i64;

        let hits = generic_redis_pool.get_key(key).await.unwrap();
        match hits {
            Some(hits) => {
                let hits = serde_json::from_str::<Vec<i64>>(&hits).unwrap();
                info!("hits: {:?}", hits);
                let (filt_hits, ret) =
                    Self::sliding_window_limiter_pure(curr_time, &hits, frame_hits_lim, frame_len);

                if !ret {
                    return Err(AppError::HitsLimitExceeded);
                }

                let _ = generic_redis_pool
                    .set_with_expiry(key, serde_json::to_string(&filt_hits).unwrap(), frame_len)
                    .await;

                Ok(filt_hits)
            }
            None => Ok(vec![]),
        }
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
