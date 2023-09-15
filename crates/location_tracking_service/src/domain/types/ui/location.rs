/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateDriverLocationRequest {
    pub pt: Point,
    pub ts: TimeStamp,
    pub acc: Option<Accuracy>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BulkDataReq {
    pub ride_id: RideId,
    pub loc: Vec<Point>,
    pub driver_id: DriverId,
}
#[derive(Serialize)]
pub struct LocationUpdate {
    pub r_id: String,
    pub m_id: MerchantId,
    pub ts: TimeStamp,
    pub st: TimeStamp,
    pub pt: Point,
    pub acc: Accuracy,
    pub ride_status: String,
    pub da: bool,
    pub mode: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum DriverRideStatus {
    PreRide,
    ActualRide,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverLocationResponse {
    pub curr_point: Point,
    pub total_distance: f32,
    pub status: DriverRideStatus,
    pub last_update: DateTime<Utc>,
}
