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
    pub group_id: Option<String>,
    pub group_id2: Option<String>,
}

pub type NearbyDriverResponse = Vec<DriverLocationDetail>;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetDriversLocationRequest {
    pub driver_ids: Vec<DriverId>,
}

pub type GetDriversLocationResponse = Vec<DriverLocationDetail>;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RideDetailsApiEntity {
    pub ride_id: RideId,
    pub ride_status: RideStatus,
    pub ride_info: Option<RideInfo>,
}

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
    pub group_id: Option<String>,
    pub group_id2: Option<String>,
    pub bear: Option<Direction>,
    pub ride_details: Option<RideDetailsApiEntity>,
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

/// A single cached special location entry (debug).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CachedSpecialLocationEntry {
    pub id: String,
    pub is_queue_enabled: bool,
    pub is_open_market_enabled: bool,
}

/// Group of cached special locations per city (debug).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CachedSpecialLocationCityGroup {
    pub merchant_operating_city_id: String,
    pub count: usize,
    pub special_locations: Vec<CachedSpecialLocationEntry>,
}

/// Response for GET /internal/special-locations/cached
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CachedSpecialLocationsResponse {
    pub total_count: usize,
    pub cities: Vec<CachedSpecialLocationCityGroup>,
}

/// Response for GET /internal/special-locations/{special_location_id}/drivers
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpecialLocationDriversResponse {
    pub driver_ids: Vec<DriverId>,
}

pub type TrackVehiclesResponse = Vec<TrackVehicleResponse>;

/// Response for GET /internal/special-locations/{special_location_id}/queue/drivers/{driver_id}/position
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DriverQueuePositionResponse {
    pub queue_position_range: Option<(u64, u64)>,
    pub queue_size: u64,
}

/// Request body for POST manual queue add
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ManualQueueAddRequest {
    pub queue_position: u64, // 1-indexed desired position
}

/// Request body for DELETE manual queue remove. Optional — callers that don't
/// send a body get `exit:manual` in the rank-history hash; callers that do
/// get `exit:manual:<reason>` so the timeline shows *why* the operator pulled
/// the driver (e.g. wrong queue, complaint, fraud).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ManualQueueRemoveRequest {
    pub reason: Option<String>,
}

/// One event from the per-driver rank-history hash. `value` is the raw event
/// string — see `driver_queue_rank_history_key` doc for the format.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DriverQueueHistoryEvent {
    pub timestamp: f64,
    pub value: String,
}

/// Snapshot of the tracking state alongside the historical timeline.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DriverQueueTrackingSnapshot {
    pub special_location_id: String,
    pub vehicle_type: String,
    pub consecutive_exit_pings: u32,
    pub last_recorded_rank: Option<u64>,
}

/// Response for GET /internal/drivers/{merchant_id}/{driver_id}/queue-history.
/// Bounded by `RANK_HISTORY_TTL_SECS` (2h) — events older than that are not
/// returned because Redis has already expired them.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DriverQueueHistoryResponse {
    pub tracking_state: Option<DriverQueueTrackingSnapshot>,
    pub current_rank: Option<u64>,
    pub events: Vec<DriverQueueHistoryEvent>,
}

/// A single driver entry in the queue
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QueueDriverEntry {
    pub driver_id: DriverId,
    pub queue_position: u64, // 1-indexed
}

/// Response for GET /internal/special-locations/{special_location_id}/queue/{vehicle_type}/drivers
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QueueDriversResponse {
    pub drivers: Vec<QueueDriverEntry>,
    pub queue_size: u64,
}
