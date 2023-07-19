use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateDriverLocationRequest {
    pub pt: Point,
    pub ts: DateTime<Utc>,
    pub acc: i32,
    pub vt: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetNearbyDriversRequest {
    pub lat: f64,
    pub lon: f64,
    pub vt: String,
    pub radius: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverLocs {
    pub lon: f64,
    pub lat: f64,
    pub driver_id: String,
}
