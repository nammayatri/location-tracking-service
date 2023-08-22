use serde::{Deserialize, Serialize};

use crate::common::types::*;

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

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideEndResponse {
    pub ride_id: String,
    pub loc: Vec<Point>,
    pub driver_id: String,
}

#[derive(Serialize, Debug)]
pub struct ResponseData {
    pub result: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DriverRideResponse {
    pub resp: Vec<(Longitude, Latitude)>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideDetailsRequest {
    pub ride_id: String,
    pub ride_status: RideStatus,
    pub merchant_id: MerchantId,
    pub driver_id: DriverId,
    pub lat: Latitude,
    pub lon: Longitude,
}
