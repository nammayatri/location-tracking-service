/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;

/// Constructs a Redis key for associating a driver ID with an authentication token.
///
/// The resulting key is intended to be stored in persistent Redis storage.
///
/// # Arguments
///
/// * `token` - The authentication token.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn set_driver_id_key(Token(token): &Token) -> String {
    format!("lts:driver_id:{token}")
}

/// Constructs a Redis key for controlling the API hit limits for a driver.
///
/// The resulting key is intended to be stored in persistent Redis storage.
///
/// # Arguments
///
/// * `driver_id` - The unique driver ID.
/// * `city` - The name of the city.
/// * `merchant_id` - The merchant ID.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn sliding_rate_limiter_key(
    DriverId(driver_id): &DriverId,
    CityName(city): &CityName,
    MerchantId(merchant_id): &MerchantId,
) -> String {
    format!("lts:ratelimit:{merchant_id}:{driver_id}:{city}")
}

/// Constructs a Redis key to set an atomic lock for a driver's location update.
///
/// The resulting key is intended to be stored in persistent Redis storage.
///
/// # Arguments
///
/// * `driver_id` - The unique driver ID.
/// * `merchant_id` - The merchant ID.
/// * `city` - The name of the city.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn driver_processing_location_update_lock_key(
    DriverId(driver_id): &DriverId,
    MerchantId(merchant_id): &MerchantId,
    CityName(city): &CityName,
) -> String {
    format!("lts:dl:processing:{merchant_id}:{driver_id}:{city}")
}

/// Constructs a Redis key for storing details about an ongoing ride.
///
/// The resulting key is intended to be stored in persistent Redis storage.
///
/// # Arguments
///
/// * `merchant_id` - The merchant ID.
/// * `driver_id` - The unique driver ID.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn on_ride_details_key(
    MerchantId(merchant_id): &MerchantId,
    DriverId(driver_id): &DriverId,
) -> String {
    format!("lts:on_ride_details:{merchant_id}:{driver_id}")
}

/// Constructs a Redis key for storing location updates during a ride.
///
/// The resulting key is intended to be stored in persistent Redis storage.
///
/// # Arguments
///
/// * `merchant_id` - The merchant ID.
/// * `driver_id` - The unique driver ID.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn on_ride_loc_key(
    MerchantId(merchant_id): &MerchantId,
    DriverId(driver_id): &DriverId,
) -> String {
    format!("lts:dl:on_ride:loc:{merchant_id}:{driver_id}")
}

/// Constructs a Redis key to retrieve details based on ride ID.
///
/// The resulting key is intended to be stored in persistent Redis storage.
///
/// # Arguments
///
/// * `ride_id` - The unique ride ID.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn on_ride_driver_details_key(RideId(ride_id): &RideId) -> String {
    format!("lts:on_ride_driver_details:{ride_id}")
}

/// Constructs a Redis key for storing ride status and city details for a driver.
///
/// The resulting key is intended to be stored in persistent Redis storage.
///
/// # Arguments
///
/// * `driver_id` - The unique driver ID.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn driver_details_key(DriverId(driver_id): &DriverId) -> String {
    format!("lts:driver_details:{driver_id}")
}

/// Constructs a Redis key specifically for health checks.
///
/// # Returns
///
/// A string representing the Redis key for health checks.
///
pub fn health_check_key() -> String {
    "lts:health_check".to_string()
}

/// Constructs a Redis key for storing a driver's location updates based on certain criteria.
///
/// The resulting key has a duration determined by the bucket and is intended for non-persistent Redis storage.
///
/// # Arguments
///
/// * `merchant_id` - The merchant ID.
/// * `city` - The name of the city.
/// * `vehicle_type` - The type of vehicle.
/// * `bucket` - The duration for which the key is valid.
///
/// # Returns
///
/// A string formatted Redis key.
///
pub fn driver_loc_bucket_key(
    MerchantId(merchant_id): &MerchantId,
    CityName(city): &CityName,
    vehicle_type: &VehicleType,
    bucket: &u64,
) -> String {
    format!("lts:dl:off_ride:loc:{merchant_id}:{vehicle_type}:{city}:{bucket}")
}

pub fn driver_loc_based_on_route_key(route_code: &str) -> String {
    format!("route:{route_code}")
}

pub fn driver_loc_based_on_trip_key(trip_id: &str) -> String {
    format!("trip:{trip_id}")
}

pub fn google_stop_duration_key(source_stop_code: &str, destination_stop_code: &str) -> String {
    format!("gsd:{source_stop_code}:{destination_stop_code}")
}

pub fn google_route_duration_cache_processing_key() -> String {
    "grd:processing".to_string()
}
