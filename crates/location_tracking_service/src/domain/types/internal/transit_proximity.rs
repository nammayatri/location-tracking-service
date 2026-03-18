/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use serde::{Deserialize, Serialize};

use crate::common::types::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransitProximityRequest {
    pub rider_location: Point,
    pub target_stop_code: String,
    pub target_stop_location: Point,
    pub route_code: String,
    pub scheduled_departure: TimeStamp,
    pub gtfs_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VehicleEta {
    pub vehicle_id: String,
    pub eta_to_stop_seconds: i64,
    pub current_delay_seconds: i64,
    pub is_live: bool,
    pub last_updated: TimeStamp,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransitProximityResponse {
    pub walking_eta_to_stop: i64,
    pub walking_distance_to_stop: f64,
    pub vehicle_eta: Option<VehicleEta>,
    pub should_leave_now: bool,
    pub advisory_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_quality: Option<String>,
}
