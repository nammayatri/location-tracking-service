/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;

///
/// This key is used to store driverId against Auth token
/// We store this key in, Persistent Redis
///
pub fn set_driver_id_key(Token(token): &Token) -> String {
    format!("lts:dl:driver_id:{token}")
}

///
/// This key is used to control API hit limits for driverId
/// We store this key in, Persistent Redis
///
pub fn sliding_rate_limiter_key(
    DriverId(driver_id): &DriverId,
    CityName(city): &CityName,
    MerchantId(merchant_id): &MerchantId,
) -> String {
    format!("lts:ratelimit:{merchant_id}:{driver_id}:{city}")
}

///
/// This key is used to set Atomic lock for driverId
/// We store this key in, Persistent Redis
///
pub fn driver_processing_location_update_lock_key(
    DriverId(driver_id): &DriverId,
    MerchantId(merchant_id): &MerchantId,
    CityName(city): &CityName,
) -> String {
    format!("lts:dl:processing:{merchant_id}:{driver_id}:{city}")
}

///
/// This key is used to store (rideId, rideStatus, city of booking) for driverId
/// We store this key in, Persistent Redis
///
pub fn on_ride_details_key(
    MerchantId(merchant_id): &MerchantId,
    DriverId(driver_id): &DriverId,
) -> String {
    format!("lts:on_ride_details:{merchant_id}:{driver_id}")
}

///
/// This key is used to store location updates from RideStart to RideEnd
/// We store this key in, Persistent Redis
///
pub fn on_ride_loc_key(
    MerchantId(merchant_id): &MerchantId,
    DriverId(driver_id): &DriverId,
) -> String {
    format!("lts:dl:on_ride:loc:{merchant_id}:{driver_id}")
}

///
/// This key is used to get (driverId, merchantId, city) based on rideId
/// We store this key in, Persistent Redis
///
pub fn on_ride_driver_details_key(RideId(ride_id): &RideId) -> String {
    format!("lts:on_ride_driver_details:{ride_id}")
}

///
/// This key is used to store ride status and city of booking for driverId
/// We store this key in, Persistent Redis
///
pub fn driver_details_key(DriverId(driver_id): &DriverId) -> String {
    format!("lts:driver_details:{driver_id}")
}

///
/// This key is used to only for health check
///
pub fn health_check_key() -> String {
    "lts:health_check".to_string()
}

///
/// This key is used to store driver location updates for (Merchant, City, Vehicle)
/// The key will be valid until the duration of the bucket
/// We store this key in, Non Persistent Redis
///
pub fn driver_loc_bucket_key(
    MerchantId(merchant_id): &MerchantId,
    CityName(city): &CityName,
    vehicle_type: &VehicleType,
    bucket: &u64,
) -> String {
    format!("lts:dl:off_ride:loc:{merchant_id}:{vehicle_type}:{city}:{bucket}")
}
