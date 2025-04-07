/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use chrono::{DateTime, Utc};
use fred::types::GeoValue;
use geo::MultiPolygon;
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
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Eq, PartialEq, PartialOrd)]
#[macros::impl_getter]
pub struct TimeStamp(pub DateTime<Utc>);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Copy)]
#[macros::impl_getter]
pub struct Radius(pub f64);
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, PartialOrd, Copy)]
#[macros::impl_getter]
pub struct Accuracy(pub f64);
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
        destination: Point,
        polyline: Option<String>,
        driver_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Car { pickup_location: Point },
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
#[serde(rename_all = "camelCase")]
pub struct RideDetails {
    pub ride_id: RideId,
    pub ride_status: RideStatus,
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
    #[strum(serialize = "DRIVER_REACHING")]
    #[serde(rename = "DRIVER_REACHING")]
    DriverReaching = 2,
    #[strum(serialize = "DRIVER_REACHED")]
    #[serde(rename = "DRIVER_REACHED")]
    DriverReached = 3,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RideBookingDetails {
    pub driver_last_known_location: DriverLastKnownLocation,
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
}

pub type ViolationDetectionTriggerMap = FxHashMap<DetectionType, Option<DetectionStatus>>;
pub type ViolationDetectionStateMap = FxHashMap<DetectionType, ViolationDetectionState>;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq, EnumIter)]
pub enum DetectionType {
    Stopped,
    RouteDeviation,
    Overspeeding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationDetectionState {
    StopDetection(StopDetectionState),
    RouteDeviation(RouteDeviationState),
    Overspeeding(OverspeedingState),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopDetectionState {
    pub avg_speed: Option<VecDeque<(f64, u32)>>,
    pub avg_coord_mean: VecDeque<(Point, u32)>,
    pub total_datapoints: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDeviationState {
    pub minimum_deviation_distance: f64,
    pub total_datapoints: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverspeedingState {
    pub total_datapoints: u64,
    pub avg_speed_record: VecDeque<(f64, u32)>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationDetectionConfig {
    pub enabled_on_pick_up: bool,
    pub enabled_on_ride: bool,
    pub detection_config: DetectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionConfig {
    StoppedDetection(StoppedDetectionConfig),
    OverspeedingDetection(OverspeedingConfig),
    RouteDeviationDetection(RouteDeviationConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionContext {
    pub driver_id: DriverId,
    pub ride_id: RideId,
    pub location: Point,
    pub timestamp: TimeStamp,
    pub speed: Option<SpeedInMeterPerSecond>,
    pub ride_status: RideStatus,
    pub ride_info: Option<RideInfo>,
    pub vehicle_type: VehicleType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionStatus {
    ContinuedViolation,
    ContinuedAntiViolation,
    Violated,
    AntiViolated,
}
