/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::common::types::*;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NearbyDriversRequest {
    pub lat: Latitude,
    pub lon: Longitude,
    pub on_ride: Option<bool>,
    pub vehicle_type: Option<VehicleType>,
    pub radius: Radius,
    pub merchant_id: MerchantId,
}

pub type NearbyDriverResponse = Vec<DriverLocation>;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetDriversLocationRequest {
    pub driver_ids: Vec<DriverId>,
}

pub type GetDriversLocationResponse = Vec<DriverLocation>;

#[derive(Serialize, Deserialize, Debug, Clone)]
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
