/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use serde::{Deserialize, Serialize};

use crate::common::types::*;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NearbyDriversRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub vehicle_type: Option<Vec<VehicleType>>,
    pub radius: Radius,
    pub merchant_id: MerchantId,
}

pub type NearbyDriverResponse = Vec<DriverLocationDetail>;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetDriversLocationRequest {
    pub driver_ids: Vec<DriverId>,
}

pub type GetDriversLocationResponse = Vec<DriverLocationDetail>;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DriverLocationDetail {
    pub driver_id: DriverId,
    pub lat: Latitude,
    pub lon: Longitude,
    pub coordinates_calculated_at: TimeStamp,
    pub created_at: TimeStamp,
    pub updated_at: TimeStamp,
    pub merchant_id: MerchantId,
    pub bear: Option<Direction>,
    pub ride_details: Option<RideDetails>,
    pub vehicle_type: Option<VehicleType>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DriverBlockTillRequest {
    pub merchant_id: MerchantId,
    pub driver_id: DriverId,
    pub block_till: TimeStamp,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum TrackVehicleRequest {
    RouteCode(String),
    TripCodes(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrackVehicleResponse {
    pub vehicle_number: String,
    pub vehicle_info: VehicleTrackingInfo,
}

pub type TrackVehiclesResponse = Vec<TrackVehicleResponse>;
