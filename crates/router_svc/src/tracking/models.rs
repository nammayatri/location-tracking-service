use crate::types::*;
use chrono::{DateTime, Utc};
use geo::MultiPolygon;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateDriverLocationRequest {
    pub pt: Point,
    pub ts: TimeStamp,
    pub acc: Accuracy,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Point {
    pub lat: Latitude,
    pub lon: Longitude,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverLocs {
    pub lon: Longitude,
    pub lat: Latitude,
    pub driver_id: DriverId,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetNearbyDriversRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub vehicle_type: VehicleType,
    pub radius: Radius,
    pub merchant_id: MerchantId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideStartRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideEndRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RideId {
    pub on_ride: bool,
    pub ride_id: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DriverRideData {
    pub resp: Vec<(Longitude, Latitude)>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DurationStruct {
    pub dur: Duration,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseData {
    pub driver_id: String,
}

#[derive(Clone)]
pub struct MultiPolygonBody {
    pub region: String,
    pub multipolygon: MultiPolygon,
}

#[derive(Serialize, Debug)]
pub struct ResponseData {
    pub result: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BulkDataReq {
    pub ride_id: String,
    pub loc: Vec<Point>,
    pub driver_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverLocation {
    pub driver_id: DriverId,
    pub lat: Latitude,
    pub lon: Longitude,
    pub coordinates_calculated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub merchant_id: MerchantId,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideEndRes {
    pub ride_id: String,
    pub loc: Vec<Point>,
    pub driver_id: String,
}
