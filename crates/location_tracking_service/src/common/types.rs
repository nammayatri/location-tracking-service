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
use serde::{Deserialize, Serialize};
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
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, PartialOrd, Copy)]
#[macros::impl_getter]
pub struct SpeedInMeterPerSecond(pub f64);
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
#[macros::impl_getter]
pub struct Token(pub String);
#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq, Copy)]
#[macros::impl_getter]
pub struct Meters(pub u32);

pub type DriversLocationMap = FxHashMap<String, Vec<GeoValue>>;

#[derive(
    Debug, Clone, EnumString, EnumIter, Display, Serialize, Deserialize, Eq, Hash, PartialEq,
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
}

#[derive(Deserialize, Serialize, Clone, Debug, Display, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RideInfo {
    #[serde(rename_all = "camelCase")]
    Bus {
        route_code: String,
        bus_number: String,
        destination: Point,
    },
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum RideStatus {
    NEW,
    INPROGRESS,
    CANCELLED, // TODO :: To be removed
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverAllDetails {
    pub driver_last_known_location: DriverLastKnownLocation,
    pub blocked_till: Option<TimeStamp>,
    pub stop_detection: Option<StopDetection>,
    pub ride_status: Option<RideStatus>,
    // pub travelled_distance: Option<Meters>,
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
    pub timestamp: TimeStamp,
    pub ride_status: Option<RideStatus>,
}
