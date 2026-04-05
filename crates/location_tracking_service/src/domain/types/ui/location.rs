/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Person type for generic location APIs (rider or driver).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersonType {
    Rider,
    Driver,
}

impl FromStr for PersonType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rider" => Ok(PersonType::Rider),
            "driver" => Ok(PersonType::Driver),
            _ => Err(()),
        }
    }
}

impl PersonType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PersonType::Rider => "rider",
            PersonType::Driver => "driver",
        }
    }
}

/// Entity type for generic APIs (ride or sos).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    Ride,
    Sos,
}

impl FromStr for EntityType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ride" => Ok(EntityType::Ride),
            "sos" => Ok(EntityType::Sos),
            _ => Err(()),
        }
    }
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Ride => "ride",
            EntityType::Sos => "sos",
        }
    }

    pub fn to_entity_id(&self, id: &str) -> EntityId {
        match self {
            EntityType::Ride => EntityId::Ride(RideId(id.to_string())),
            EntityType::Sos => EntityId::Sos(SosId(id.to_string())),
        }
    }
}

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
#[serde(rename_all = "camelCase")]
pub struct UpdateRiderLocationRequest {
    pub pt: Point,
    pub acc: Option<Accuracy>,
    pub v: Option<SpeedInMeterPerSecond>,
    pub bear: Option<Direction>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RiderLocationResponse {
    pub curr_point: Point,
    pub last_update: TimeStamp,
}

/// Unified location update request for generic person (rider/driver) APIs. Same shape as driver.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePersonLocationRequest {
    pub pt: Point,
    pub ts: TimeStamp,
    pub acc: Option<Accuracy>,
    pub v: Option<SpeedInMeterPerSecond>,
    pub bear: Option<Direction>,
}

/// Unified location response for generic person (rider/driver) track. Same as DriverLocationResponse / RiderLocationResponse.
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PersonLocationResponse {
    pub curr_point: Point,
    pub last_update: TimeStamp,
    pub accuracy: Option<Accuracy>,
}
