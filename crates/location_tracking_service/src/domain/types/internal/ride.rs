/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use serde::{Deserialize, Serialize};

use crate::common::types::*;
use crate::outbound::types::LocationUpdate;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideCreateRequest {
    pub merchant_id: MerchantId,
    pub driver_id: DriverId,
    pub is_future_ride: Option<bool>,
    pub ride_info: Option<RideInfo>,
    pub ride_pickup_location: Option<Point>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideStartRequest {
    pub merchant_id: MerchantId,
    pub driver_id: DriverId,
    pub ride_info: Option<RideInfo>,
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
    pub ts: Option<i64>,
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
    pub next_ride_id: Option<RideId>,
    pub ride_info: Option<RideInfo>,
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
    pub loc: Vec<LocationUpdate>,
    pub timestamp: Option<TimeStamp>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideEndResponse {
    pub ride_id: RideId,
    pub loc: Vec<LocationUpdate>,
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
    pub ride_info: Option<RideInfo>,
}

// --- Generic entity upsert (rider/driver) types ---

/// Union type: entity create (optional, not for SOS), start tracking, or end tracking.
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "entityInfo", rename_all = "camelCase")]
pub enum EntityInfo {
    EntityCreate,
    EntityStart,
    EntityEnd { lat: Latitude, lon: Longitude },
}

/// Request body for entity upsert (create/start/end). person_type is in the URL path.
/// For `EntityStart`, include `broadcaster_config` to atomically register the broadcaster
/// alongside the entity — eliminating the need for a separate setExternalConfig call.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EntityUpsertRequest {
    pub person_id: String,
    pub merchant_id: MerchantId,
    pub entity_info: EntityInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broadcaster_config: Option<BroadcasterConfigRequest>,
}

/// Response for entity upsert: success for create/start, or batched locations for end.
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum EntityUpsertResponse {
    APISuccess(crate::common::types::APISuccess),
    EntityEnd { loc: Vec<Point> },
}

/// Request body for registering a broadcaster config. Sent as part of `EntityStart`.
/// The `provider` field carries provider-kind discriminant + provider-specific fields.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BroadcasterConfigRequest {
    pub external_reference_id: String,
    pub base_url: String,
    pub access_token: String,
    pub token_expires_at: i64,
    pub ny_reauth_url: String,
    pub ny_api_key: String,
    pub merchant_operating_city_id: String,
    pub polling_interval_secs: u32,
    pub time_diff_secs: i64,
    pub expires_at: Option<i64>,
    pub provider: TraceProvider,
}

/// Response from an external provider reauth endpoint.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExternalReauthResponse {
    pub access_token: String,
    pub expires_at: i64,
}
