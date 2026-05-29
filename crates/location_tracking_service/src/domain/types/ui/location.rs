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

/// Person type for generic location APIs (rider, driver, or bus crew).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonType {
    Rider,
    Driver,
    BusConductor,
    BusDriver,
}

impl FromStr for PersonType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rider" => Ok(PersonType::Rider),
            "driver" => Ok(PersonType::Driver),
            "bus_conductor" => Ok(PersonType::BusConductor),
            "bus_driver" => Ok(PersonType::BusDriver),
            _ => Err(()),
        }
    }
}

impl PersonType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PersonType::Rider => "rider",
            PersonType::Driver => "driver",
            PersonType::BusConductor => "bus_conductor",
            PersonType::BusDriver => "bus_driver",
        }
    }

    /// True for bus crew pings (`bus_conductor` / `bus_driver`) that are
    /// forwarded to the per-fleet Kafka topic instead of running the
    /// driver location pipeline.
    pub fn is_bus_crew(&self) -> bool {
        matches!(self, PersonType::BusConductor | PersonType::BusDriver)
    }

    /// `(person_type, provider)` strings carried in the forwarded Kafka payload.
    pub fn bus_kafka_tags(&self) -> Option<(&'static str, &'static str)> {
        match self {
            PersonType::BusConductor => Some(("bus_conductor", "lts-bus-conductor")),
            PersonType::BusDriver => Some(("bus_driver", "lts-bus-driver")),
            _ => None,
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
    /// Optional. When set to `"bus_conductor"` / `"bus_driver"` together with
    /// `gtfs_id` and `vehicle_no`, the handler forwards the ping to the Kafka
    /// topic configured for that fleet instead of running the driver flow.
    /// Absent → existing driver path.
    pub person_type: Option<PersonType>,
    pub gtfs_id: Option<String>,
    pub vehicle_no: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn person_type_wire_values_round_trip() {
        let cases = [
            (PersonType::Rider, "\"rider\""),
            (PersonType::Driver, "\"driver\""),
            (PersonType::BusConductor, "\"bus_conductor\""),
            (PersonType::BusDriver, "\"bus_driver\""),
        ];
        for (variant, wire) in cases {
            assert_eq!(serde_json::to_string(&variant).ok(), Some(wire.to_string()));
            assert_eq!(
                serde_json::from_str::<PersonType>(wire).ok(),
                Some(variant),
                "deserialize {wire}"
            );
            // FromStr (path params) must agree with the serde wire value.
            let bare = wire.trim_matches('"');
            assert_eq!(PersonType::from_str(bare).ok(), Some(variant));
            assert_eq!(variant.as_str(), bare);
        }
    }

    #[test]
    fn legacy_conductor_value_is_rejected() {
        // `conductor` was intentionally removed; it must no longer parse.
        assert!(serde_json::from_str::<PersonType>("\"conductor\"").is_err());
        assert!(PersonType::from_str("conductor").is_err());
    }

    #[test]
    fn bus_kafka_tags_only_for_bus_crew() {
        assert_eq!(
            PersonType::BusConductor.bus_kafka_tags(),
            Some(("bus_conductor", "lts-bus-conductor"))
        );
        assert_eq!(
            PersonType::BusDriver.bus_kafka_tags(),
            Some(("bus_driver", "lts-bus-driver"))
        );
        assert_eq!(PersonType::Driver.bus_kafka_tags(), None);
        assert_eq!(PersonType::Rider.bus_kafka_tags(), None);
        assert!(PersonType::BusConductor.is_bus_crew());
        assert!(PersonType::BusDriver.is_bus_crew());
        assert!(!PersonType::Driver.is_bus_crew());
    }
}
