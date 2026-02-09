/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::utils::serialize_url;
use crate::environment::deserialize_url;
use chrono::{DateTime, Utc};
use fred::types::GeoValue;
use geo::MultiPolygon;
use reqwest::Url;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::VecDeque;
use strum_macros::{Display, EnumIter, EnumString};

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
#[macros::impl_getter]
pub struct RideId(pub String);
#[derive(Deserialize, Serialize, Clone, Debug, Eq, Hash, PartialEq)]
#[macros::impl_getter]
pub struct DriverId(pub String);
#[derive(Deserialize, Serialize, Clone, Debug, Eq, Hash, PartialEq)]
#[macros::impl_getter]
pub struct MerchantId(pub String);
#[derive(Deserialize, Serialize, Clone, Debug, Eq, Hash, PartialEq)]
#[macros::impl_getter]
pub struct MerchantOperatingCityId(pub String);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Copy)]
#[macros::impl_getter]
pub struct Latitude(pub f64);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Copy)]
#[macros::impl_getter]
pub struct Direction(pub f64);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Copy)]
#[macros::impl_getter]
pub struct Longitude(pub f64);
#[derive(Deserialize, Serialize, Clone, Debug, Eq, Hash, PartialEq)]
#[macros::impl_getter]
pub struct CityName(pub String);
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Hash, Ord)]
#[macros::impl_getter]
pub struct TimeStamp(pub DateTime<Utc>);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Copy)]
#[macros::impl_getter]
pub struct Radius(pub f64);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, PartialOrd, Copy)]
#[macros::impl_getter]
pub struct Accuracy(pub f64);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, PartialOrd, Copy)]
#[macros::impl_getter]
pub struct Seconds(pub u32);
#[derive(Serialize, Clone, Debug, PartialEq, PartialOrd, Copy)]
#[macros::impl_getter]
pub struct SpeedInMeterPerSecond(pub f64);
impl<'de> Deserialize<'de> for SpeedInMeterPerSecond {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{Error, Unexpected};

        struct SpeedVisitor;

        #[allow(clippy::needless_lifetimes)]
        impl<'de> serde::de::Visitor<'de> for SpeedVisitor {
            type Value = SpeedInMeterPerSecond;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a number (integer/float) or a string containing a floating-point number",
                )
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(SpeedInMeterPerSecond(value))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
                Ok(SpeedInMeterPerSecond(v as f64))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
                Ok(SpeedInMeterPerSecond(v as f64))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                value
                    .parse::<f64>()
                    .map(SpeedInMeterPerSecond)
                    .map_err(|_| Error::invalid_value(Unexpected::Str(value), &self))
            }
        }

        deserializer.deserialize_any(SpeedVisitor)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
#[macros::impl_getter]
pub struct Token(pub String);
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq, Copy)]
#[macros::impl_getter]
pub struct Meters(pub u32);

pub type DriversLocationMap = FxHashMap<String, Vec<GeoValue>>;

#[derive(
    Debug, Clone, EnumString, EnumIter, Display, Serialize, Deserialize, Eq, Hash, PartialEq, Copy,
)]
pub enum VehicleType {
    #[strum(serialize = "AUTO_RICKSHAW")]
    #[serde(rename = "AUTO_RICKSHAW")]
    AutoRickshaw,
    SEDAN,
    SUV,
    HATCHBACK,
    TAXI,
    BIKE,
    #[strum(serialize = "DELIVERY_BIKE")]
    #[serde(rename = "DELIVERY_BIKE")]
    DeliveryBike,
    #[strum(serialize = "TAXI_PLUS")]
    #[serde(rename = "TAXI_PLUS")]
    TaxiPlus,
    #[strum(serialize = "AUTO_PLUS")]
    #[serde(rename = "AUTO_PLUS")]
    AutoPlus,
    #[strum(serialize = "PREMIUM_SEDAN")]
    #[serde(rename = "PREMIUM_SEDAN")]
    PremiumSedan,
    BLACK,
    #[strum(serialize = "BLACK_XL")]
    #[serde(rename = "BLACK_XL")]
    BlackXl,
    #[strum(serialize = "SUV_PLUS")]
    #[serde(rename = "SUV_PLUS")]
    SuvPlus,
    #[strum(serialize = "AMBULANCE_TAXI")]
    #[serde(rename = "AMBULANCE_TAXI")]
    AmbulanceTaxi,
    #[strum(serialize = "AMBULANCE_TAXI_OXY")]
    #[serde(rename = "AMBULANCE_TAXI_OXY")]
    AmbulanceTaxiOxy,
    #[strum(serialize = "AMBULANCE_AC")]
    #[serde(rename = "AMBULANCE_AC")]
    AmbulanceAc,
    #[strum(serialize = "AMBULANCE_AC_OXY")]
    #[serde(rename = "AMBULANCE_AC_OXY")]
    AmbulanceAcOxy,
    #[strum(serialize = "AMBULANCE_VENTILATOR")]
    #[serde(rename = "AMBULANCE_VENTILATOR")]
    AmbulanceVentilator,
    #[strum(serialize = "BUS_AC")]
    #[serde(rename = "BUS_AC")]
    BusAc,
    #[strum(serialize = "BUS_NON_AC")]
    #[serde(rename = "BUS_NON_AC")]
    BusNonAc,
    #[strum(serialize = "EV_AUTO_RICKSHAW")]
    #[serde(rename = "EV_AUTO_RICKSHAW")]
    EvAutoRickshaw,
    #[strum(serialize = "HERITAGE_CAB")]
    #[serde(rename = "HERITAGE_CAB")]
    HeritageCab,
    #[strum(serialize = "DELIVERY_TRUCK_MINI")]
    #[serde(rename = "DELIVERY_TRUCK_MINI")]
    DeliveryTruckMini,
    #[strum(serialize = "DELIVERY_TRUCK_SMALL")]
    #[serde(rename = "DELIVERY_TRUCK_SMALL")]
    DeliveryTruckSmall,
    #[strum(serialize = "DELIVERY_TRUCK_MEDIUM")]
    #[serde(rename = "DELIVERY_TRUCK_MEDIUM")]
    DeliveryTruckMedium,
    #[strum(serialize = "DELIVERY_TRUCK_LARGE")]
    #[serde(rename = "DELIVERY_TRUCK_LARGE")]
    DeliveryTruckLarge,
    #[strum(serialize = "DELIVERY_TRUCK_ULTRA_LARGE")]
    #[serde(rename = "DELIVERY_TRUCK_ULTRA_LARGE")]
    DeliveryTruckUltraLarge,
    #[strum(serialize = "VIP_ESCORT")]
    #[serde(rename = "VIP_ESCORT")]
    VipEscort,
    #[strum(serialize = "VIP_OFFICER")]
    #[serde(rename = "VIP_OFFICER")]
    VipOfficer,
    #[strum(serialize = "BIKE_PLUS")]
    #[serde(rename = "BIKE_PLUS")]
    BikePlus,
    #[strum(serialize = "AC_PRIORITY")]
    #[serde(rename = "AC_PRIORITY")]
    AcPriority,
    #[strum(serialize = "E_RICKSHAW")]
    #[serde(rename = "E_RICKSHAW")]
    ERickShaw,
}

#[derive(Deserialize, Serialize, Clone, Debug, Display, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RideInfo {
    #[serde(rename_all = "camelCase")]
    Bus {
        started_at: Option<TimeStamp>,
        route_code: String,
        route_long_name: Option<String>,
        bus_number: String,
        source: Option<Point>,
        destination: Point,
        driver_name: Option<String>,
        group_id: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Car {
        pickup_location: Point,
        min_distance_between_two_points: Option<i32>,
        ride_stops: Option<Vec<Point>>,
    },
    #[serde(rename_all = "camelCase")]
    Pilot {
        destination: Point,
        driver_name: Option<String>,
        duty_type: Option<String>,
        end_address: Option<String>,
        group_id: Option<String>,
        pilot_number: String,
        scheduled_trip_time: Option<TimeStamp>,
        source: Point,
        start_address: Option<String>,
        vip_name: Option<String>,
    },
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum RideStatus {
    NEW,
    INPROGRESS,
    CANCELLED, // TODO :: To be removed
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum LocationType {
    UNFILTERED,
    FILTERED,
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum DriverMode {
    ONLINE,
    OFFLINE,
    SILENT,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct APISuccess {
    result: String,
}

impl Default for APISuccess {
    fn default() -> Self {
        Self {
            result: "Success".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuthData {
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
    pub merchant_operating_city_id: MerchantOperatingCityId,
}

pub struct DriverLocationPoint {
    pub driver_id: DriverId,
    pub location: Point,
}

#[derive(Clone)]
pub struct MultiPolygonBody {
    pub region: String,
    pub multipolygon: MultiPolygon,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct RideDetails {
    #[serde(alias = "rideId")]
    pub ride_id: RideId,
    #[serde(alias = "rideStatus")]
    pub ride_status: RideStatus,
    #[serde(alias = "rideInfo")]
    pub ride_info: Option<RideInfo>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct DriverDetails {
    pub driver_id: DriverId, // TODO :: Make it string from json to save deserialization cost.
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DriversRideStatus {
    pub driver_id: DriverId,
    pub location: Point,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Point {
    pub lat: Latitude,
    pub lon: Longitude,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Dimensions {
    pub merchant_id: MerchantId,
    pub city: CityName,
    pub vehicle_type: VehicleType,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverLastKnownLocation {
    pub location: Point,
    pub timestamp: TimeStamp,
    pub merchant_id: MerchantId,
    pub bear: Option<Direction>,
    pub vehicle_type: Option<VehicleType>,
    pub group_id: Option<String>,
    pub group_id2: Option<String>,
    pub driver_mode: Option<DriverMode>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverLocation {
    pub location: Point,
    pub timestamp: TimeStamp,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StopDetection {
    pub locations: VecDeque<DriverLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDeviation {
    pub locations: VecDeque<DriverLocation>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverAllDetails {
    pub driver_last_known_location: DriverLastKnownLocation,
    pub blocked_till: Option<TimeStamp>,
    pub stop_detection: Option<StopDetection>,
    pub ride_status: Option<RideStatus>,
    pub ride_notification_status: Option<RideNotificationStatus>,
    pub driver_pickup_distance: Option<Meters>,
    pub violation_trigger_flag: Option<ViolationDetectionTriggerMap>,
    pub detection_state: Option<ViolationDetectionStateMap>,
    pub anti_detection_state: Option<ViolationDetectionStateMap>,
    pub group_id: Option<String>,
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Clone,
    Copy,
    EnumIter,
    EnumString,
    Display,
)]
pub enum RideNotificationStatus {
    #[strum(serialize = "IDLE")]
    #[serde(rename = "IDLE")]
    Idle = 0,
    #[strum(serialize = "DRIVER_ON_THE_WAY")]
    #[serde(rename = "DRIVER_ON_THE_WAY")]
    DriverOnTheWay = 1,
    #[strum(serialize = "DRIVER_PICKUP_INSTRUCTION")]
    #[serde(rename = "DRIVER_PICKUP_INSTRUCTION")]
    DriverPickupInstruction = 2,
    #[strum(serialize = "DRIVER_REACHING")]
    #[serde(rename = "DRIVER_REACHING")]
    DriverReaching = 3,
    #[strum(serialize = "DRIVER_REACHED")]
    #[serde(rename = "DRIVER_REACHED")]
    DriverReached = 4,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RideBookingDetails {
    pub driver_last_known_location: DriverLastKnownLocation,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NextStopInfo {
    pub name: String,
    pub distance: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VehicleTrackingInfo {
    pub start_time: Option<TimeStamp>,
    pub schedule_relationship: Option<String>,
    pub trip_id: Option<String>,
    pub latitude: Latitude,
    pub longitude: Longitude,
    pub speed: Option<SpeedInMeterPerSecond>,
    pub timestamp: Option<TimeStamp>,
    pub ride_status: Option<RideStatus>,
    pub upcoming_stops: Option<Vec<UpcomingStop>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpcomingStop {
    pub stop: Stop,
    pub eta: TimeStamp,
    pub status: UpcomingStopStatus,
    pub delta: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum UpcomingStopStatus {
    Reached,
    Upcoming,
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq, Copy)]
pub enum TravelMode {
    #[strum(serialize = "DRIVE")]
    #[serde(rename = "DRIVE")]
    Drive,
    #[strum(serialize = "WALK")]
    #[serde(rename = "WALK")]
    Walk,
    #[strum(serialize = "BICYCLE")]
    #[serde(rename = "BICYCLE")]
    Bicycle,
    #[strum(serialize = "TRANSIT")]
    #[serde(rename = "TRANSIT")]
    Transit,
    #[strum(serialize = "TWO_WHEELER")]
    #[serde(rename = "TWO_WHEELER")]
    TwoWheeler,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteProperties {
    #[serde(rename = "Route Code")]
    pub route_code: String,
    #[serde(rename = "Travel Mode")]
    pub travel_mode: TravelMode,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StopProperties {
    #[serde(rename = "Stop Name")]
    pub stop_name: String,
    #[serde(rename = "Stop Code")]
    pub stop_code: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteFeature {
    pub geometry: RouteGeometry,
    pub properties: RouteProperties,
    #[serde(rename = "type")]
    pub feature_type: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StopFeature {
    pub geometry: StopGeometry,
    pub properties: StopProperties,
    #[serde(rename = "type")]
    pub feature_type: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteGeometry {
    pub coordinates: Vec<Vec<f64>>,
    #[serde(rename = "type")]
    pub geometry_type: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StopGeometry {
    pub coordinates: Vec<f64>,
    #[serde(rename = "type")]
    pub geometry_type: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteGeoJSON {
    pub features: Vec<serde_json::Value>,
    #[serde(rename = "type")]
    pub geo_type: String,
}

#[derive(Debug, Clone)]
pub struct WaypointInfo {
    pub coordinate: Point,
    pub stop: Stop,
}

#[derive(Debug, Clone)]
pub struct Route {
    pub route_code: String,
    pub travel_mode: TravelMode,
    pub waypoints: Vec<WaypointInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Stop {
    pub name: String,
    pub stop_code: String,
    pub coordinate: Point,
    pub stop_idx: usize,
    pub distance_to_upcoming_intermediate_stop: Meters,
    pub duration_to_upcoming_intermediate_stop: Seconds,
    pub distance_from_previous_intermediate_stop: Meters,
    pub stop_type: StopType,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum StopType {
    IntermediateStop,
    RouteCorrectionStop,
    UpcomingStop,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpcomingIntermediateStop {
    pub name: String,
    pub coordinate: Point,
}

#[derive(Debug)]
pub struct ProjectionPoint {
    pub segment_index: i32,
    pub projection_point: Point,
    pub projection_point_to_point_distance: f64,
    pub projection_point_to_line_start_distance: f64,
    pub projection_point_to_line_end_distance: f64,
}

pub type ViolationDetectionTriggerMap = FxHashMap<DetectionType, Option<DetectionStatus>>;
pub type ViolationDetectionStateMap = FxHashMap<DetectionType, ViolationDetectionState>;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, EnumIter)]
pub enum DetectionType {
    Stopped,
    RouteDeviation,
    Overspeeding,
    OppositeDirection,
    TripNotStarted,
    SafetyCheck,
    RideStopReached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationDetectionState {
    StopDetection(StopDetectionState),
    RouteDeviation(RouteDeviationState),
    Overspeeding(OverspeedingState),
    OppositeDirection(OppositeDirectionState),
    TripNotStarted(TripNotStartedState),
    SafetyCheck(SafetyCheckState),
    RideStopReached(RideStopReachedState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopDetectionState {
    pub avg_speed: Option<VecDeque<(f64, u32)>>,
    pub avg_coord_mean: VecDeque<(Point, u32)>,
    pub total_datapoints: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDeviationState {
    pub deviation_distance: f64,
    pub total_datapoints: u64,
    pub avg_deviation_record: VecDeque<(Point, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverspeedingState {
    pub total_datapoints: u64,
    pub avg_speed_record: VecDeque<(f64, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OppositeDirectionState {
    pub total_datapoints: u64,
    pub expected_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripNotStartedState {
    pub total_datapoints: u64,
    pub avg_coord_mean: VecDeque<(Point, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyCheckState {
    pub avg_speed: Option<VecDeque<(f64, u32)>>,
    pub avg_coord_mean: VecDeque<(Point, u32)>,
    pub total_datapoints: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RideStopReachedState {
    pub total_datapoints: u64,
    pub reached_stops: Vec<String>,
    pub current_stop_index: usize,
    pub avg_coord_mean: VecDeque<(Point, u32)>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoppedDetectionConfig {
    pub batch_count: u32,
    pub sample_size: u32,
    pub max_eligible_speed: Option<SpeedInMeterPerSecond>,
    pub max_eligible_distance: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverspeedingConfig {
    pub speed_limit: f64,
    pub sample_size: u32,
    pub batch_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDeviationConfig {
    pub deviation_threshold: u32,
    pub sample_size: u32,
    pub batch_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OppositeDirectionConfig {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripNotStartedConfig {
    pub deviation_threshold: u32,
    pub sample_size: u32,
    pub batch_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationDetectionConfig {
    pub enabled: bool,
    #[serde(deserialize_with = "deserialize_url", serialize_with = "serialize_url")]
    pub detection_callback_url: Url,
    pub detection_config: DetectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionConfig {
    StoppedDetection(StoppedDetectionConfig),
    OverspeedingDetection(OverspeedingConfig),
    RouteDeviationDetection(RouteDeviationConfig),
    OppositeDirectionDetection(OppositeDirectionConfig),
    TripNotStartedDetection(TripNotStartedConfig),
    SafetyCheckDetection(SafetyCheckConfig),
    RideStopReachedDetection(RideStopReachedConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyCheckConfig {
    pub batch_count: u32,
    pub sample_size: u32,
    pub max_eligible_speed: Option<SpeedInMeterPerSecond>,
    pub max_eligible_distance: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RideStopReachedConfig {
    pub sample_size: u32,
    pub batch_count: u32,
    pub stop_reach_threshold: u32,
    pub min_stop_duration: u32,
}

#[derive(Debug, Clone)]
pub struct DetectionContext<'a> {
    pub driver_id: DriverId,
    pub ride_id: RideId,
    pub location: Point,
    pub timestamp: TimeStamp,
    pub speed: Option<SpeedInMeterPerSecond>,
    pub ride_status: RideStatus,
    pub ride_info: Option<RideInfo>,
    pub vehicle_type: VehicleType,
    pub accuracy: Accuracy,
    pub route: Option<&'a Route>,
    pub ride_stops: Option<&'a Vec<Point>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionStatus {
    ContinuedViolation,
    ContinuedAntiViolation,
    Violated,
    AntiViolated,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct DriverByPlateResp {
    pub driver_id: String,
    pub merchant_id: String,
    pub bus_number: Option<String>,
    pub group_id: Option<String>,
    pub vehicle_service_tier_type: VehicleType,
}
