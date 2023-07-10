use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateDriverLocationRequest {
    pt: Point,
    ts: DateTime<Utc>,
    acc: i32,
    vt: String,
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
