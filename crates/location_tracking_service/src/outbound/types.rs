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

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TriggerStatusFcmReq {
    pub ride_id: RideId,
    pub driver_id: DriverId,
    pub ride_notification_status: RideNotificationStatus,
}

// Trigger Stop Detection Event
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StopDetectionReq {
    pub location: Point,
    pub ride_id: RideId,
    pub driver_id: DriverId,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverReachedDestinationReq {
    pub location: Point,
    pub ride_id: RideId,
    pub driver_id: DriverId,
    pub vehicle_variant: VehicleType,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverSourceDepartedReq {
    pub location: Point,
    pub ride_id: RideId,
    pub driver_id: DriverId,
    pub vehicle_variant: VehicleType,
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

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RouteDeviationReq {
    pub location: Point,
    pub ride_id: RideId,
    pub driver_id: DriverId,
    pub deviation: f64,
}

// Trigger Stop Detection Event
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StoppedDetectionData {
    pub location: Point,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RouteDeviationDetectionData {
    pub location: Point,
    pub distance: f64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OverSpeedingDetectionData {
    pub location: Point,
    pub speed: f64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TripNotStartedDetectionData {
    pub location: Point,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OppositeDirectionDetectionData {
    pub location: Point,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum DetectionData {
    RouteDeviationDetection(RouteDeviationDetectionData),
    StoppedDetection(StoppedDetectionData),
    OverSpeedingDetection(OverSpeedingDetectionData),
    TripNotStartedDetection(TripNotStartedDetectionData),
    OppositeDirectionDetection(OppositeDirectionDetectionData),
    SafetyCheckDetection(SafetyCheckDetectionData),
    RideStopReachedDetection(RideStopReachedDetectionData),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ViolationDetectionReq {
    pub ride_id: RideId,
    pub driver_id: DriverId,
    pub is_violated: bool,
    pub detection_data: DetectionData,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GoogleRoutesRequest {
    pub origin: Location,
    pub destination: Location,
    pub intermediates: Vec<Location>,
    pub travel_mode: TravelMode,
    pub routing_preference: String,
    pub compute_alternative_routes: bool,
    pub extra_computations: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub location: OuterLatLng,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OuterLatLng {
    pub lat_lng: LatLng,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LatLng {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GoogleRoutesResponse {
    pub routes: Vec<Route>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub distance_meters: i32,
    pub duration: String,
    pub polyline: Polyline,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Polyline {
    pub encoded_polyline: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OsrmDistanceMatrixResponse {
    pub code: String,
    pub distances: Vec<Vec<f64>>,
    pub durations: Vec<Vec<f64>>,
    pub sources: Vec<OsrmWaypoint>,
    pub destinations: Vec<OsrmWaypoint>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OsrmWaypoint {
    pub distance: f64,
    pub name: String,
    pub location: Vec<f64>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SafetyCheckDetectionData {
    pub location: Point,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RideStopReachedDetectionData {
    pub location: Point,
    pub stop_name: String,
    pub stop_code: String,
    pub stop_index: usize,
    pub reached_at: TimeStamp,
}
