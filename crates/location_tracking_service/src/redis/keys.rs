/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use crate::domain::types::ui::location::PersonType;

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

/// Constructs a Redis key for caching driver information by vehicle plate number.
/// This cache is created at ride start and cleaned up at ride end.
/// Used by external GPS provider integration to map plate numbers to driver IDs.
pub fn driver_info_by_plate_key(plate_number: &str) -> String {
    format!("lts:driver_info:plate:{}", plate_number)
}

// --- Generic person/entity keys (for unified rider flow and future driver unification) ---

/// Token → person identifier.
pub fn identifier_key(person_type: PersonType, Token(token): &Token) -> String {
    format!("lts:identifier:{}:{token}", person_type.as_str())
}

/// Rate limit for a person.
pub fn person_rate_limit_key(
    MerchantId(merchant_id): &MerchantId,
    PersonId(person_id): &PersonId,
    CityName(city): &CityName,
) -> String {
    format!("lts:ratelimit:{merchant_id}:{person_id}:{city}")
}

/// Entity details for a person (current entity type, id, status).
pub fn entity_details_key(
    MerchantId(merchant_id): &MerchantId,
    person_type: PersonType,
    PersonId(person_id): &PersonId,
) -> String {
    format!(
        "lts:entity_details:{merchant_id}:{}:{person_id}",
        person_type.as_str()
    )
}

/// Person by entity lookup. entity_type: "ride" | "sos", entity_id: the id string.
pub fn person_detail_by_entity_key(entity_type: &str, entity_id: &str) -> String {
    format!("lts:person_detail_by_entity:{entity_type}:{entity_id}")
}

/// Person detail (last location, status).
pub fn person_detail_key(person_type: PersonType, PersonId(person_id): &PersonId) -> String {
    format!("lts:person_detail:{}:{person_id}", person_type.as_str())
}

/// Entity location list (vector of points).
pub fn entity_loc_key(
    MerchantId(merchant_id): &MerchantId,
    person_type: PersonType,
    PersonId(person_id): &PersonId,
) -> String {
    format!(
        "lts:entity_loc:{merchant_id}:{}:{person_id}",
        person_type.as_str()
    )
}

/// Processing lock for person location update.
pub fn person_processing_lock_key(
    MerchantId(merchant_id): &MerchantId,
    person_type: PersonType,
    PersonId(person_id): &PersonId,
    CityName(city): &CityName,
) -> String {
    format!(
        "lts:processing:{merchant_id}:{}:{person_id}:{city}",
        person_type.as_str()
    )
}

/// Key for ZSET of driver_ids in a special location (per bucket).
/// Used when enable_special_location_bucketing is true.
pub fn special_location_drivers_key(special_location_id: &str, bucket: &u64) -> String {
    format!("lts:special_loc:{}:{}", special_location_id, bucket)
}

/// Key for the FIFO queue ZSET of a special location per vehicle type.
/// Score = entry timestamp, member = driver_id JSON string.
pub fn special_location_queue_key(special_location_id: &str, vehicle_type: &str) -> String {
    format!(
        "lts:special_loc_queue:{}:{}",
        special_location_id, vehicle_type
    )
}

/// Stores the earliest server timestamp seen for a driver in a special-location
/// queue (per vehicle type). Used to keep the queue ZSET score stable across
/// pods and brief out-of-fence pings: a re-entry within TTL re-uses the
/// stored timestamp, so the driver retains their original rank.
pub fn driver_queue_last_ts_key(
    special_location_id: &str,
    vehicle_type: &str,
    driver_id: &str,
) -> String {
    format!(
        "lts:queue_last_ts:{}:{}:{}",
        special_location_id, vehicle_type, driver_id
    )
}

/// Tracking key to know which queue a driver is currently in.
/// STRING storing JSON-encoded DriverQueueTracking value.
pub fn driver_queue_tracking_key(merchant_id: &str, driver_id: &str) -> String {
    format!("lts:driver_queue:{}:{}", merchant_id, driver_id)
}

/// Per-driver rolling hash of recent queue rank events. HASH field =
/// server-side ping timestamp (stringified). Value is one of:
///   - `enter:<rank>` — driver was present in the queue at that timestamp
///     and the post-write ZRANK was `<rank>`. Only written when the rank
///     actually changes from the previously recorded value, so a stationary
///     driver pinging at rank N once produces a single entry, not one per
///     ping.
///   - `exit:hysteresis` — drainer evicted the driver after N consecutive
///     out-of-geofence pings hit the hysteresis threshold.
///   - `exit:switch` — drainer evicted the driver from this queue because
///     they entered a different special location's queue on this ping.
///   - `exit:manual` or `exit:manual:<reason>` — driver was removed via the
///     internal manual-remove API (operator action), not by drainer logic.
///     Reason suffix is the free-form `reason` field from the request body
///     when supplied.
///     Bounded by a short TTL — observability only, not source-of-truth state.
pub fn driver_queue_rank_history_key(merchant_id: &str, driver_id: &str) -> String {
    format!("lts:driver_queue_rank_hist:{}:{}", merchant_id, driver_id)
}
