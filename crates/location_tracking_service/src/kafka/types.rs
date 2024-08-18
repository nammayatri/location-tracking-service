/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use serde::Serialize;

#[derive(Serialize, Clone, PartialEq)]
pub enum DriverRideStatus {
    #[serde(rename = "ON_RIDE")]
    OnRide,
    #[serde(rename = "ON_PICKUP")]
    OnPickup,
    IDLE,
}

#[derive(Serialize)]
pub struct LocationUpdate {
    pub r_id: Option<RideId>,
    pub m_id: MerchantId,
    pub pt: Point,
    pub da: bool,
    pub rid: Option<RideId>,
    pub mocid: MerchantOperatingCityId,
    pub ts: TimeStamp,
    pub st: TimeStamp,
    pub lat: Latitude,
    pub lon: Longitude,
    pub acc: Accuracy,
    pub speed: SpeedInMeterPerSecond,
    pub ride_status: DriverRideStatus,
    pub active: bool,
    pub on_ride: bool,
    pub mode: DriverMode,
    pub vehicle_variant: VehicleType,
    pub travelled_distance: Meters,
}
