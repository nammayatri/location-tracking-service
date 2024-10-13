/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use serde::{Deserialize, Serialize};

use crate::common::types::*;
use std::collections::HashMap;

// BPP Authentication
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseData {
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
    pub merchant_operating_city_id: MerchantOperatingCityId,
}

// Bulk location update during the ride
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BulkDataReq {
    pub ride_id: RideId,
    pub loc: Vec<Point>,
    pub driver_id: DriverId,
}

// Trigger FCM to start location pings if not sent for a while
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TriggerFcmReq {
    pub ride_id: RideId,
    pub driver_id: DriverId,
}

// Trigger Stop Detection Event
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StopDetectionReq {
    pub location: Point,
    pub total_locations: usize,
}

// Live activity notification trigger for IOS
#[derive(Serialize, Debug)]
pub struct TriggerApnsReq {
    pub aps: Apns,
}

#[derive(Serialize, Debug)]
pub struct Apns {
    pub timestamp: u64,
    pub event: String,
    pub content_state: HashMap<String, String>,
    pub alert: HashMap<String, serde_json::Value>,
}
