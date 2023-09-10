/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use chrono::{DateTime, Utc};
use geo::MultiPolygon;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};

pub type RideId = String;
pub type DriverId = String;
pub type MerchantId = String;
pub type Latitude = f64;
pub type Longitude = f64;
pub type CityName = String;
pub type TimeStamp = DateTime<Utc>;
pub type Radius = f64;
pub type Accuracy = i32;
pub type Token = String;

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
    #[strum(serialize = "TAXI_PLUS")]
    #[serde(rename = "TAXI_PLUS")]
    TaxiPlus,
}

#[derive(Debug, Clone, EnumString, Display, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum RideStatus {
    NEW,
    INPROGRESS,
    COMPLETED,
    CANCELLED,
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
#[serde(rename_all = "camelCase")]
pub struct AuthData {
    #[serde(rename = "driverId")]
    pub driver_id: String,
}

pub struct DriverLocationPoint {
    pub driver_id: String,
    pub location: Point,
}

#[derive(Clone)]
pub struct MultiPolygonBody {
    pub region: String,
    pub multipolygon: MultiPolygon,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct RideDetails {
    pub ride_id: String,
    pub ride_status: RideStatus,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct DriverDetails {
    pub driver_id: DriverId,
    pub merchant_id: MerchantId,
    pub city: CityName,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DriversRideStatus {
    pub driver_id: DriverId,
    pub ride_status: Option<RideStatus>,
    pub location: Point,
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriverModeDetails {
    pub driver_id: DriverId,
    pub driver_mode: DriverMode,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Point {
    pub lat: Latitude,
    pub lon: Longitude,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct Dimensions {
    pub merchant_id: MerchantId,
    pub city: CityName,
    pub vehicle_type: VehicleType,
    pub on_ride: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverLastKnownLocation {
    pub location: Point,
    pub timestamp: DateTime<Utc>,
    pub merchant_id: MerchantId,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverAllDetails {
    pub driver_last_known_location: Option<DriverLastKnownLocation>,
    pub driver_mode: Option<DriverMode>,
}
