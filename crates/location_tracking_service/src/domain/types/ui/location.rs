/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateDriverLocationRequest {
    pub pt: Point,
    pub ts: TimeStamp,
    pub acc: Option<Accuracy>,
    pub v: Option<SpeedInMeterPerSecond>,
    pub bear: Option<Direction>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverLocationResponse {
    pub curr_point: Point,
    pub last_update: TimeStamp,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateRiderSosLocationRequest {
    pub sos_id: SosId,
    pub pt: Point,
    pub acc: Option<Accuracy>,
    pub v: Option<SpeedInMeterPerSecond>,
    pub bear: Option<Direction>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RiderSosLocationResponse {
    pub curr_point: Point,
    pub last_update: TimeStamp,
}
