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

#[derive(Serialize, Deserialize, Clone)]
pub struct GetNearbyDriversRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub vehicle_type: VehicleType,
    pub radius: Radius,
    pub merchant_id: MerchantId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverLocs {
    pub lon: Longitude,
    pub lat: Latitude,
    pub driver_id: DriverId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RideStartRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RideEndRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct NearbyDriverResp {
    pub resp: Vec<DriverLocation>,
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
pub struct AuthResponseData {
    pub driverId: String,
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
pub struct BulkDataReq {
    pub rideId: String,
    pub loc: Vec<Point>,
    pub driverId: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DriverLocation {
    pub driverId: DriverId,
    pub lat: Latitude,
    pub lon: Longitude,
    pub coordinatesCalculatedAt: DateTime<Utc>,
    pub createdAt: DateTime<Utc>,
    pub updatedAt: DateTime<Utc>,
    pub merchantId: MerchantId,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RideEndRes {
    pub rideId: String,
    pub loc: Vec<Point>,
    pub driverId: String,
}
