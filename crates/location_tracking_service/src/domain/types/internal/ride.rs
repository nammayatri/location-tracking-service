/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use serde::{Deserialize, Serialize};

use crate::common::types::*;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideCreateRequest {
    pub merchant_id: MerchantId,
    pub driver_id: DriverId,
    pub vehicle_number: String,
    pub ride_start_otp: u32,
    pub estimated_pickup_distance: Meters,
    pub is_future_ride: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideStartRequest {
    pub merchant_id: MerchantId,
    pub driver_id: DriverId,
}

#[derive(Serialize, Debug)]
pub struct ResponseData {
    pub result: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideEndRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
    pub next_ride_id: Option<RideId>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverLocationRequest {
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverLocationResponse {
    pub loc: Vec<Point>,
    pub timestamp: Option<TimeStamp>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideEndResponse {
    pub ride_id: RideId,
    pub loc: Vec<Point>,
    pub driver_id: DriverId,
}

// TODO :: To be deprecated...
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideDetailsRequest {
    pub ride_id: RideId,
    pub ride_status: RideStatus,
    pub is_future_ride: Option<bool>,
    pub merchant_id: MerchantId,
    pub driver_id: DriverId,
    pub lat: Latitude,
    pub lon: Longitude,
}
