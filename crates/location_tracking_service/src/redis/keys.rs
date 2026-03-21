/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

//! Redis key patterns optimized for cluster hash slot distribution.
//!
//! Entity-centric keys use Redis hash tags `{id}` so that all keys for the
//! same driver/person/ride land on the same hash slot. This enables potential
//! future use of MULTI/EXEC and reduces cross-slot splits in MGET batches.
//!
//! Distribution keys (geo buckets, special locations) intentionally omit hash
//! tags so entries spread evenly across cluster nodes.

use crate::common::types::*;
use crate::domain::types::ui::location::PersonType;

// ---------------------------------------------------------------------------
// Driver / auth keys — hash-tagged on driver_id or token
// ---------------------------------------------------------------------------

/// Token → driver auth data. Hash tag on token.
pub fn set_driver_id_key(Token(token): &Token) -> String {
    format!("lts:driver_id:{{{token}}}")
}

/// Rate limiter for a driver. Hash tag on driver_id.
pub fn sliding_rate_limiter_key(
    DriverId(driver_id): &DriverId,
    CityName(city): &CityName,
    MerchantId(merchant_id): &MerchantId,
) -> String {
    format!("lts:ratelimit:{merchant_id}:{{{driver_id}}}:{city}")
}

/// Lock for driver location update processing. Hash tag on driver_id.
pub fn driver_processing_location_update_lock_key(
    DriverId(driver_id): &DriverId,
    MerchantId(merchant_id): &MerchantId,
    CityName(city): &CityName,
) -> String {
    format!("lts:dl:processing:{merchant_id}:{{{driver_id}}}:{city}")
}

/// On-ride details (ride_id, status, vehicle info). Hash tag on driver_id.
pub fn on_ride_details_key(
    MerchantId(merchant_id): &MerchantId,
    DriverId(driver_id): &DriverId,
) -> String {
    format!("lts:on_ride_details:{merchant_id}:{{{driver_id}}}")
}

/// Location updates during a ride. Hash tag on driver_id.
pub fn on_ride_loc_key(
    MerchantId(merchant_id): &MerchantId,
    DriverId(driver_id): &DriverId,
) -> String {
    format!("lts:dl:on_ride:loc:{merchant_id}:{{{driver_id}}}")
}

/// Driver details by ride_id. Hash tag on ride_id.
pub fn on_ride_driver_details_key(RideId(ride_id): &RideId) -> String {
    format!("lts:on_ride_driver_details:{{{ride_id}}}")
}

/// Driver last known location + status. Hash tag on driver_id.
pub fn driver_details_key(DriverId(driver_id): &DriverId) -> String {
    format!("lts:driver_details:{{{driver_id}}}")
}

/// Health check key (no hash tag needed).
pub fn health_check_key() -> String {
    "lts:health_check".to_string()
}

// ---------------------------------------------------------------------------
// Distribution keys — NO hash tags for even cluster spread
// ---------------------------------------------------------------------------

/// Geo bucket for off-ride driver locations. No hash tag — spreads across nodes.
pub fn driver_loc_bucket_key(
    MerchantId(merchant_id): &MerchantId,
    CityName(city): &CityName,
    vehicle_type: &VehicleType,
    bucket: &u64,
) -> String {
    format!("lts:dl:off_ride:loc:{merchant_id}:{vehicle_type}:{city}:{bucket}")
}

/// Route-based vehicle tracking hash. No hash tag.
pub fn driver_loc_based_on_route_key(route_code: &str) -> String {
    format!("route:{route_code}")
}

/// Trip-based vehicle tracking hash. No hash tag.
pub fn driver_loc_based_on_trip_key(trip_id: &str) -> String {
    format!("trip:{trip_id}")
}

/// Google Maps stop duration cache. No hash tag.
pub fn google_stop_duration_key(source_stop_code: &str, destination_stop_code: &str) -> String {
    format!("gsd:{source_stop_code}:{destination_stop_code}")
}

/// Route duration cache processing lock. No hash tag.
pub fn google_route_duration_cache_processing_key() -> String {
    "grd:processing".to_string()
}

/// Driver info by plate number. Hash tag on plate_number.
pub fn driver_info_by_plate_key(plate_number: &str) -> String {
    format!("lts:driver_info:plate:{{{plate_number}}}")
}

// ---------------------------------------------------------------------------
// Generic person/entity keys — hash-tagged on person_id or token
// ---------------------------------------------------------------------------

/// Token → person auth data. Hash tag on token.
pub fn identifier_key(person_type: PersonType, Token(token): &Token) -> String {
    format!("lts:identifier:{}:{{{token}}}", person_type.as_str())
}

/// Rate limit for a person. Hash tag on person_id.
pub fn person_rate_limit_key(
    MerchantId(merchant_id): &MerchantId,
    PersonId(person_id): &PersonId,
    CityName(city): &CityName,
) -> String {
    format!("lts:ratelimit:{merchant_id}:{{{person_id}}}:{city}")
}

/// Entity details for a person. Hash tag on person_id.
pub fn entity_details_key(
    MerchantId(merchant_id): &MerchantId,
    person_type: PersonType,
    PersonId(person_id): &PersonId,
) -> String {
    format!(
        "lts:entity_details:{merchant_id}:{}:{{{person_id}}}",
        person_type.as_str()
    )
}

/// Person by entity lookup. Hash tag on entity_id.
pub fn person_detail_by_entity_key(entity_type: &str, entity_id: &str) -> String {
    format!("lts:person_detail_by_entity:{entity_type}:{{{entity_id}}}")
}

/// Person detail (last location, status). Hash tag on person_id.
pub fn person_detail_key(person_type: PersonType, PersonId(person_id): &PersonId) -> String {
    format!("lts:person_detail:{}:{{{person_id}}}", person_type.as_str())
}

/// Entity location list. Hash tag on person_id.
pub fn entity_loc_key(
    MerchantId(merchant_id): &MerchantId,
    person_type: PersonType,
    PersonId(person_id): &PersonId,
) -> String {
    format!(
        "lts:entity_loc:{merchant_id}:{}:{{{person_id}}}",
        person_type.as_str()
    )
}

/// Processing lock for person location update. Hash tag on person_id.
pub fn person_processing_lock_key(
    MerchantId(merchant_id): &MerchantId,
    person_type: PersonType,
    PersonId(person_id): &PersonId,
    CityName(city): &CityName,
) -> String {
    format!(
        "lts:processing:{merchant_id}:{}:{{{person_id}}}:{city}",
        person_type.as_str()
    )
}

/// ZSET of drivers in a special location (per bucket). No hash tag — distribution key.
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

/// Tracking key to know which queue a driver is currently in.
/// STRING storing JSON-encoded DriverQueueTracking value.
pub fn driver_queue_tracking_key(merchant_id: &str, driver_id: &str) -> String {
    format!("lts:driver_queue:{}:{}", merchant_id, driver_id)
}
