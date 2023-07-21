use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateDriverLocationRequest {
    pt: Point,
    ts: DateTime<Utc>,
    acc: i32,
    vt: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuthResponseData {
    pub driverId: String,
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Point {
    lat: f64,
    lon: f64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetNearbyDriversRequest {
    lat: f64,
    lon: f64,
    vt: String,
    radius: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RideStartRequest {
    lat: f64,
    lon: f64,
    pub driver_id: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RideEndRequest {
    lat: f64,
    lon: f64,
    pub driver_id: String,
}
