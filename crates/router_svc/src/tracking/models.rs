// use fred::types::GeoPosition;
use crate::types::*;
use serde::{Deserialize, Serialize};

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
    pub resp: Vec<(Longitude, Latitude, DriverId)>,
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
