use chrono::{DateTime, Utc};
use geo::MultiPolygon;
use serde::{Deserialize, Serialize};
use shared::redis::interface::types::RedisConnectionPool;
use shared::utils::logger::*;
use std::{collections::HashMap, sync::{Arc, Mutex}, time::{UNIX_EPOCH, SystemTime}};

pub type VehicleType = String;
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
}

impl AppState {
    pub async fn sliding_window_limiter(
        &self,
        key: &str,
        frame_hits_lim: usize,
        frame_len: u32,
        redis_pool: &std::sync::MutexGuard<'_, RedisConnectionPool>,
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