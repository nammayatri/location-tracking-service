use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::types::*;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NearbyDriversRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub vehicle_type: VehicleType,
    pub radius: Radius,
    pub merchant_id: MerchantId,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct NearbyDriverResponse {
    pub resp: Vec<DriverLocation>,
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
