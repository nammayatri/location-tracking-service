use chrono::{DateTime, Utc};
use diesel::sql_types::Array;
use serde::{Deserialize, Serialize};
use geo::{MultiPolygon};

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateDriverLocationRequest {
    pub pt: Point,
    pub ts: DateTime<Utc>,
    pub acc: i32,


}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuthResponseData {
    pub driverId: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
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

pub struct PolygonBody {
    pub region: String,
    pub polygon: Vec<(f64, f64)>
}


#[derive(Serialize, Deserialize, Clone)]
pub struct MultiPolygonBody {
    pub region: String,
    pub multipolygon: Vec<Vec<(f64,f64)>>
}