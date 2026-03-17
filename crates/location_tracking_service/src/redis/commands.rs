/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use crate::domain::types::ui::location::PersonType;
use crate::outbound::types::LocationUpdate;
use crate::redis::keys::*;
use crate::tools::error::AppError;
use fred::prelude::SortedSetsInterface;
use fred::types::{GeoPosition, GeoUnit, SetOptions, SortOrder};
use futures::Future;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use shared::redis::types::{RedisConnectionPool, Ttl};
use std::collections::HashMap;
use tracing::{error, info};

/// Sets the ride details (i.e, rideId and rideStatus) to the Redis store.
///
/// This function serializes the ride details into a JSON string and stores it
/// in the Redis store using the given key.
///
/// # Arguments
/// * `redis` - A connection pool to the Redis store.
/// * `redis_expiry` - The expiration time for the Redis key.
/// * `merchant_id` - The ID of the merchant.
/// * `driver_id` - The ID of the driver.
/// * `ride_id` - The ID of the ride.
/// * `ride_status` - The current status of the ride.
/// * `ride_info` - Ride related details based on vehicle category.
///
/// # Returns
/// * A Result indicating the success or failure of the operation.
#[allow(clippy::too_many_arguments)]
pub async fn set_ride_details_for_driver(
    redis: &RedisConnectionPool,
    redis_expiry: &u32,
    merchant_id: &MerchantId,
    driver_id: &DriverId,
    ride_id: RideId,
    ride_status: RideStatus,
    ride_info: Option<RideInfo>,
) -> Result<(), AppError> {
    let ride_details = RideDetails {
        ride_id,
        ride_status,
        ride_info: ride_info.clone(),
    };
    redis
        .set_key(
            &on_ride_details_key(merchant_id, driver_id),
            ride_details,
            *redis_expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    // For bus rides with external GPS, cache reverse mapping: plate_number -> driver_info
    if let Some(RideInfo::Bus {
        bus_number,
        group_id,
        ..
    }) = ride_info
    {
        let driver_info = DriverByPlateResp {
            driver_id: driver_id.inner(),
            merchant_id: merchant_id.inner(),
            bus_number: Some(bus_number.clone()),
            group_id: group_id.clone(),
            vehicle_service_tier_type: VehicleType::BusNonAc, // Default - GPS provider should send actual type
        };

        redis
            .set_key(
                &driver_info_by_plate_key(&bus_number),
                driver_info,
                *redis_expiry,
            )
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;
    }

    Ok(())
}

/// Fetches the details of a specific ride from the Redis database.
///
/// This function looks up the ride details for a given merchant and driver
/// in the persistent Redis database. If the ride details exist, they are
/// deserialized from a JSON string into a `RideDetails` object.
///
/// # Arguments
///
/// * `redis` - A reference to the Redis connection pool.
/// * `driver_id` - A reference to the ID of the driver.
/// * `merchant_id` - A reference to the ID of the merchant.
///
/// # Returns
///
/// A `Result` which is:
///
/// * `Ok(Some(RideDetails))` if the ride details are found and successfully deserialized.
/// * `Ok(None)` if the ride details are not found.
/// * `Err(AppError::DeserializationError)` if there's an error during deserialization.
pub async fn get_ride_details(
    redis: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
) -> Result<Option<RideDetails>, AppError> {
    redis
        .get_key::<RideDetails>(&on_ride_details_key(merchant_id, driver_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn get_all_driver_ride_details(
    redis: &RedisConnectionPool,
    driver_ids: &[DriverId],
    merchant_id: &MerchantId,
) -> Result<Vec<Option<RideDetails>>, AppError> {
    let driver_ride_details_keys = driver_ids
        .iter()
        .map(|driver_id| on_ride_details_key(merchant_id, driver_id))
        .collect::<Vec<String>>();

    redis
        .mget_keys::<RideDetails>(driver_ride_details_keys)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Cleans up the ride details for a given merchant, driver, and ride from the Redis store.
///
/// This function deletes several keys associated with the ride from the Redis store, including:
/// * Ride details specific to the merchant and driver.
/// * Driver details specific to the ride.
/// * Location details specific to the merchant and driver.
///
/// It is typically invoked after the completion or cancellation of a ride to remove any
/// temporary data associated with that ride.
///
/// # Arguments
/// * `redis` - A reference to the connection pool for the Redis store.
/// * `merchant_id` - A reference to the ID of the merchant.
/// * `driver_id` - A reference to the ID of the driver.
/// * `ride_id` - A reference to the ID of the ride.
///
/// # Returns
/// * A `Result` with an empty tuple `()` if the cleanup was successful, or an `AppError`
///   indicating the error that occurred.
///
/// # Errors
/// This function will return an `AppError` if there's a failure in deleting the keys
/// from the Redis store.
pub async fn ride_cleanup(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    driver_id: &DriverId,
    ride_id: &RideId,
    ride_info: &Option<RideInfo>,
) -> Result<(), AppError> {
    redis
        .delete_keys(vec![
            &on_ride_details_key(merchant_id, driver_id),
            &on_ride_driver_details_key(ride_id),
            &on_ride_loc_key(merchant_id, driver_id),
        ])
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    if let Some(RideInfo::Bus {
        route_code,
        bus_number,
        ..
    }) = ride_info
    {
        redis
            .hdel(&driver_loc_based_on_route_key(route_code), bus_number)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        // Clean up driver info cache (plate number mapping)
        redis
            .delete_key(&driver_info_by_plate_key(bus_number))
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;
    }

    Ok(())
}

/// Stores the driver details associated with a specific ride into the Redis database.
///
/// This function serializes the `DriverDetails` object into a JSON string and stores it
/// in the persistent Redis database using a key generated from the provided `ride_id`.
/// The stored details also have an associated expiry time.
///
/// # Arguments
///
/// * `redis` - A reference to the Redis connection pool.
/// * `redis_expiry` - A reference to the expiration time (in seconds) for the stored data.
/// * `ride_id` - A reference to the ID of the ride for which driver details are being stored.
/// * `driver_details` - The details of the driver to be stored (driverId).
///
/// # Returns
///
/// A `Result` which is:
///
/// * `Ok(())` if the driver details are successfully stored.
/// * `Err(AppError::SerializationError)` if there's an error during serialization.
pub async fn set_on_ride_driver_details(
    redis: &RedisConnectionPool,
    redis_expiry: &u32,
    ride_id: &RideId,
    driver_details: DriverDetails,
) -> Result<(), AppError> {
    redis
        .set_key(
            &on_ride_driver_details_key(ride_id),
            driver_details,
            *redis_expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Retrieves driver details (driverId) associated with a specific ride.
///
/// This function queries a Redis datastore using the ride's ID as the key.
/// It then deserializes the JSON string from Redis into a `DriverDetails` struct.
///
/// # Parameters
/// - `redis`: A connection pool to the Redis datastore.
/// - `ride_id`: The ID of the ride whose driver details we want to fetch.
///
/// # Returns
/// - `Ok(Some(DriverDetails))` if the details are found and successfully deserialized.
/// - `Ok(None)` if no details are found for the given ride ID.
/// - `Err(AppError)` if there's an issue during the fetch or deserialization process.
pub async fn get_driver_details(
    redis: &RedisConnectionPool,
    ride_id: &RideId,
) -> Result<Option<DriverDetails>, AppError> {
    redis
        .get_key::<DriverDetails>(&on_ride_driver_details_key(ride_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub async fn get_drivers_within_radius(
    redis: &RedisConnectionPool,
    nearby_bucket_threshold: &u64,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    location: Point,
    Radius(radius): &Radius,
) -> Result<Vec<DriverLocationPoint>, AppError> {
    let Latitude(lat) = location.lat;
    let Longitude(lon) = location.lon;

    let bucket_keys: Vec<String> = (0..*nearby_bucket_threshold)
        .map(|bucket_idx| driver_loc_bucket_key(merchant_id, city, vehicle, &(bucket - bucket_idx)))
        .collect();

    let nearby_drivers: Vec<(DriverId, Point)> = redis
        .mgeo_search(
            bucket_keys,
            GeoPosition::from((lon, lat)),
            (*radius, GeoUnit::Meters),
            SortOrder::Asc,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?
        .into_iter()
        .map(|(driver_id, point)| {
            (
                DriverId(driver_id),
                Point {
                    lat: Latitude(point.lat),
                    lon: Longitude(point.lon),
                },
            )
        })
        .collect();

    info!("Get Nearby Drivers {:?}", nearby_drivers);

    let mut driver_ids: FxHashSet<DriverId> = FxHashSet::default();
    let mut resp: Vec<DriverLocationPoint> = Vec::with_capacity(nearby_drivers.len());

    for (driver_id, location) in nearby_drivers.into_iter() {
        if !(driver_ids.contains(&driver_id)) {
            driver_ids.insert(driver_id.to_owned());
            resp.push(DriverLocationPoint {
                driver_id,
                location,
            })
        }
    }

    Ok(resp)
}

/// Fetches the last known location of a driver.
///
/// Queries the Redis datastore using the driver's ID and retrieves their last known location.
/// If the driver's details aren't found or the location isn't available, returns `None`.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `driver_id` - Unique identifier of the driver whose location is being fetched.
///
/// # Returns
///
/// A `Result` wrapping the driver's last known location as `Option<DriverLastKnownLocation>`,
/// or an `AppError` in case of deserialization failure.
pub async fn get_driver_location(
    redis: &RedisConnectionPool,
    driver_id: &DriverId,
) -> Result<Option<DriverAllDetails>, AppError> {
    redis
        .get_key::<DriverAllDetails>(&driver_details_key(driver_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Updates and stores the last known location of a driver.
///
/// This function creates a record of the driver's last known location, serializes it,
/// and then stores the serialized data in a Redis datastore.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `last_location_timstamp_expiry` - Expiry duration (in seconds) for the last known location record in Redis.
/// * `driver_id` - Unique identifier of the driver whose location is being updated.
/// * `merchant_id` - Identifier for the merchant associated with the driver.
/// * `last_location_pt` - Geographical point representing the driver's last known location.
/// * `last_location_ts` - Timestamp of when the driver was last at the specified location.
///
/// # Returns
///
/// A `Result` wrapping the driver's updated last known location (`DriverLastKnownLocation`),
/// or an `AppError` in case of serialization failure.
#[allow(clippy::too_many_arguments)]
pub async fn set_driver_last_location_update(
    redis: &RedisConnectionPool,
    last_location_timstamp_expiry: &u32,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    last_location_pt: &Point,
    last_location_ts: &TimeStamp,
    blocked_till: &Option<TimeStamp>,
    stop_detection: Option<StopDetection>,
    ride_status: &Option<RideStatus>,
    ride_notification_status: &Option<RideNotificationStatus>,
    detection_state: &Option<ViolationDetectionStateMap>,
    anti_detection_state: &Option<ViolationDetectionStateMap>,
    violation_trigger_flag: &Option<ViolationDetectionTriggerMap>,
    driver_pickup_distance: &Option<Meters>,
    bear: &Option<Direction>,
    vehicle_type: &Option<VehicleType>,
    group_id: &Option<String>,
    group_id2: &Option<String>,
) -> Result<DriverLastKnownLocation, AppError> {
    let last_known_location = DriverLastKnownLocation {
        location: Point {
            lat: last_location_pt.lat,
            lon: last_location_pt.lon,
        },
        timestamp: *last_location_ts,
        merchant_id: merchant_id.to_owned(),
        bear: *bear,
        vehicle_type: *vehicle_type,
        group_id: group_id.clone(),
        group_id2: group_id2.clone(),
    };

    let value = DriverAllDetails {
        driver_last_known_location: last_known_location.to_owned(),
        blocked_till: blocked_till.to_owned(),
        stop_detection,
        ride_status: ride_status.to_owned(),
        ride_notification_status: *ride_notification_status,
        driver_pickup_distance: *driver_pickup_distance,
        detection_state: detection_state.to_owned(),
        violation_trigger_flag: violation_trigger_flag.clone(),
        anti_detection_state: anti_detection_state.clone(),
        group_id: group_id.clone(),
    };

    redis
        .set_key(
            &driver_details_key(driver_id),
            value,
            *last_location_timstamp_expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(last_known_location)
}

/// Pushes a list of geographical locations to the end of a Redis list for a specific driver on a ride.
///
/// This function serializes each geographical point, then appends them to the end of a Redis list.
/// The list represents the locations visited by a driver during an active ride.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `driver_id` - Unique identifier of the driver.
/// * `merchant_id` - Identifier for the merchant associated with the driver.
/// * `geo_entries` - A slice of geographical points representing the locations to be added.
/// * `rpush_expiry` - Expiry duration (in seconds) for the list in Redis.
///
/// # Returns
///
/// A `Result` wrapping the length of the Redis list after the push operation (`i64`),
/// or an `AppError` in case of failures.
pub async fn push_on_ride_driver_locations(
    redis: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    geo_entries: Vec<LocationUpdate>,
    rpush_expiry: &u32,
) -> Result<i64, AppError> {
    redis
        .rpush_with_expiry(
            &on_ride_loc_key(merchant_id, driver_id),
            geo_entries,
            *rpush_expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Retrieves the count of geographical locations for a driver during an active ride from Redis.
///
/// This function fetches the length of the Redis list that stores the locations visited by
/// a driver during an active ride. This list contains serialized geographical points.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `driver_id` - Unique identifier of the driver.
/// * `merchant_id` - Identifier for the merchant associated with the driver.
///
/// # Returns
///
/// A `Result` wrapping the length of the Redis list representing the count of geographical locations (`i64`),
/// or an `AppError` in case of failures.
pub async fn get_on_ride_driver_locations_count(
    redis: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
) -> Result<i64, AppError> {
    redis
        .llen(&on_ride_loc_key(merchant_id, driver_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Fetches the driver's geographical locations during an active ride from Redis, and deletes it.
///
/// Retrieves a specified number of geographical points, based on the length parameter,
/// that were stored in Redis for a driver during an active ride.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `driver_id` - Unique identifier of the driver.
/// * `merchant_id` - Identifier for the merchant associated with the driver.
/// * `len` - The number of points to retrieve.
///
/// # Returns
///
/// A `Result` wrapping a vector of geographical points (`Vec<Point>`), or an `AppError` in case of failures.
pub async fn get_on_ride_driver_locations_and_delete(
    redis: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    len: i64,
) -> Result<Vec<LocationUpdate>, AppError> {
    redis
        .lpop::<LocationUpdate>(&on_ride_loc_key(merchant_id, driver_id), Some(len as usize))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Fetches the driver's geographical locations during an active ride from Redis.
///
/// Retrieves a specified number of geographical points, based on the length parameter,
/// that were stored in Redis for a driver during an active ride.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `driver_id` - Unique identifier of the driver.
/// * `merchant_id` - Identifier for the merchant associated with the driver.
/// * `len` - The number of points to retrieve.
///
/// # Returns
///
/// A `Result` wrapping a vector of geographical points (`Vec<Point>`), or an `AppError` in case of failures.
pub async fn get_on_ride_driver_locations(
    redis: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    len: i64,
) -> Result<Vec<LocationUpdate>, AppError> {
    redis
        .lrange::<LocationUpdate>(&on_ride_loc_key(merchant_id, driver_id), 0, len)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Caches a driver's unique identifier (driverId) with a given authentication token in Redis.
///
/// Stores the driver's identifier associated with an authentication token for a specified expiry period.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `auth_token_expiry` - Duration of time (in seconds) for which the driver ID should be associated with the token.
/// * `token` - The authentication token.
/// * `DriverId(driver_id)` - The unique driver identifier.
///
/// # Returns
///
/// A `Result` indicating success or an `AppError` in case of failures.
pub async fn set_driver_id(
    redis: &RedisConnectionPool,
    auth_token_expiry: &u32,
    token: &Token,
    driver_id: DriverId,
    merchant_id: MerchantId,
    merchant_operating_city_id: MerchantOperatingCityId,
) -> Result<(), AppError> {
    let auth_data = AuthData {
        driver_id,
        merchant_id,
        merchant_operating_city_id,
    };
    redis
        .set_key(&set_driver_id_key(token), auth_data, *auth_token_expiry)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Retrieves a driver's unique identifier (driverId) associated with a given authentication token from Redis.
///
/// Fetches the driver's identifier that was previously stored in Redis in association with an authentication token.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `token` - The authentication token.
///
/// # Returns
///
/// A `Result` wrapping the driver's unique identifier or `None` if not found, or an `AppError` in case of failures.
pub async fn get_driver_id(
    redis: &RedisConnectionPool,
    token: &Token,
) -> Result<Option<AuthData>, AppError> {
    redis
        .get_key::<AuthData>(&set_driver_id_key(token))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

// --- Generic person/entity Redis commands (unified keys) ---

/// Gets person auth data by token.
pub async fn get_person_identifier(
    redis: &RedisConnectionPool,
    person_type: PersonType,
    token: &Token,
) -> Result<Option<PersonAuthData>, AppError> {
    redis
        .get_key::<PersonAuthData>(&identifier_key(person_type, token))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Sets person identifier (token → auth data).
pub async fn set_person_identifier(
    redis: &RedisConnectionPool,
    person_type: PersonType,
    token: &Token,
    auth_data: &PersonAuthData,
    expiry: &u32,
) -> Result<(), AppError> {
    redis
        .set_key(&identifier_key(person_type, token), auth_data, *expiry)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Gets entity details for a person (current entity_id, merchant_id).
pub async fn get_entity_details_for_person(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    person_type: PersonType,
    person_id: &PersonId,
) -> Result<Option<PersonEntityDetails>, AppError> {
    redis
        .get_key::<PersonEntityDetails>(&entity_details_key(merchant_id, person_type, person_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Sets entity details for a person.
pub async fn set_entity_details_for_person(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    person_type: PersonType,
    person_id: &PersonId,
    details: &PersonEntityDetails,
    expiry: &u32,
) -> Result<(), AppError> {
    redis
        .set_key(
            &entity_details_key(merchant_id, person_type, person_id),
            details,
            *expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Gets person ID by entity (entity_type: "ride" | "sos", entity_id: id string).
pub async fn get_person_by_entity(
    redis: &RedisConnectionPool,
    entity_type: &str,
    entity_id: &str,
) -> Result<Option<String>, AppError> {
    redis
        .get_key::<String>(&person_detail_by_entity_key(entity_type, entity_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Sets person ↔ entity mapping (person_id string stored by entity).
pub async fn set_person_by_entity(
    redis: &RedisConnectionPool,
    entity_type: &str,
    entity_id: &str,
    person_id: &PersonId,
    expiry: &u32,
) -> Result<(), AppError> {
    redis
        .set_key(
            &person_detail_by_entity_key(entity_type, entity_id),
            person_id.inner().clone(),
            *expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Gets person detail (last location, etc.).
pub async fn get_person_detail(
    redis: &RedisConnectionPool,
    person_type: PersonType,
    person_id: &PersonId,
) -> Result<Option<PersonAllDetails>, AppError> {
    redis
        .get_key::<PersonAllDetails>(&person_detail_key(person_type, person_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Sets person detail (last known location).
pub async fn set_person_detail(
    redis: &RedisConnectionPool,
    person_type: PersonType,
    person_id: &PersonId,
    details: &PersonAllDetails,
    expiry: &u32,
) -> Result<(), AppError> {
    redis
        .set_key(&person_detail_key(person_type, person_id), details, *expiry)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Pushes locations to entity location list (vector, same as driver on_ride).
pub async fn push_entity_locations(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    person_type: PersonType,
    person_id: &PersonId,
    geo_entries: Vec<Point>,
    rpush_expiry: &u32,
) -> Result<i64, AppError> {
    redis
        .rpush_with_expiry(
            &entity_loc_key(merchant_id, person_type, person_id),
            geo_entries,
            *rpush_expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Gets entity location list length.
pub async fn get_entity_locations_count(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    person_type: PersonType,
    person_id: &PersonId,
) -> Result<i64, AppError> {
    redis
        .llen(&entity_loc_key(merchant_id, person_type, person_id))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Gets entity locations and optionally removes them (for entity end).
pub async fn get_entity_locations(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    person_type: PersonType,
    person_id: &PersonId,
    len: i64,
) -> Result<Vec<Point>, AppError> {
    redis
        .lrange::<Point>(&entity_loc_key(merchant_id, person_type, person_id), 0, len)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Gets entity locations and deletes them (pop).
pub async fn get_entity_locations_and_delete(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    person_type: PersonType,
    person_id: &PersonId,
    len: i64,
) -> Result<Vec<Point>, AppError> {
    redis
        .lpop::<Point>(
            &entity_loc_key(merchant_id, person_type, person_id),
            Some(len as usize),
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Deletes all generic entity keys for a person (entity_details, person_detail_by_entity, entity_loc, person_detail).
pub async fn entity_cleanup_generic(
    redis: &RedisConnectionPool,
    merchant_id: &MerchantId,
    person_type: PersonType,
    person_id: &PersonId,
    entity_type: &str,
    entity_id: &str,
) -> Result<(), AppError> {
    let keys = [
        entity_details_key(merchant_id, person_type, person_id),
        person_detail_by_entity_key(entity_type, entity_id),
        entity_loc_key(merchant_id, person_type, person_id),
        person_detail_key(person_type, person_id),
    ];
    redis
        .delete_keys(keys.iter().map(String::as_str).collect::<Vec<&str>>())
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Executes a callback function while maintaining a lock in Redis.
///
/// Ensures that a specific operation represented by the callback function can be performed atomically by acquiring
/// a lock in Redis. If the lock is successfully acquired, the callback function is executed; otherwise, an error is returned.
///
/// # Arguments
///
/// * `redis` - A connection pool to the Redis datastore.
/// * `key` - The key in Redis for which the lock needs to be acquired.
/// * `expiry` - Duration of time (in seconds) for which the lock should be held.
/// * `callback` - The callback function to execute once the lock is acquired.
/// * `args` - Arguments to be passed to the callback function.
///
/// # Returns
///
/// A `Result` indicating success or an `AppError` in case of failures, specifically if the lock limit is exceeded.
pub async fn with_lock_redis<F, Args, Fut, T>(
    redis: &RedisConnectionPool,
    key: String,
    expiry: i64,
    callback: F,
    args: Args,
) -> Result<T, AppError>
where
    F: Fn(Args) -> Fut,
    Args: Send + 'static,
    Fut: Future<Output = Result<T, AppError>>,
{
    // TODO :: This lock can be made optimistic by using counter approach
    match redis
        .ttl(&key)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?
    {
        Ttl::NoExpiry | Ttl::NoKeyFound => {
            match redis
                .setnx_with_expiry(&key, true, expiry)
                .await
                .map_err(|err| AppError::InternalError(err.to_string()))
            {
                Ok(true) => {
                    info!("Got lock : {}", &key);
                    let resp = callback(args).await;
                    redis
                        .delete_key(&key)
                        .await
                        .map_err(|err| AppError::InternalError(err.to_string()))?;
                    info!("Released lock : {}", &key);
                    resp
                }
                Ok(false) => Err(AppError::UnderProcessing(key)),
                Err(err) => {
                    error!("[Error] setnx_with_expiry : {:?}", err);
                    let _ = redis
                        .delete_key(&key)
                        .await
                        .map_err(|err| AppError::InternalError(err.to_string()));
                    Err(AppError::InternalError(err.to_string()))
                }
            }
        }
        Ttl::TtlValue(_) => Err(AppError::UnderProcessing(key)),
    }
}

/// Retrieve the last known locations for a list of drivers.
///
/// This function takes a reference to a Redis connection pool and a slice of driver IDs,
/// and returns a vector of optional last known locations for each driver. If there's an
/// error in retrieving or deserializing the data for a driver, `None` is returned for that driver.
///
/// # Arguments
///
/// * `redis` - A reference to the Redis connection pool.
/// * `driver_ids` - A slice containing the IDs of the drivers whose locations are to be fetched.
///
/// # Returns
///
/// A `Result` which is:
///
/// * `Ok(Vec<Option<DriverLastKnownLocation>>)` - A vector of optional last known locations for each driver.
/// * `Err(AppError)` - An error indicating what went wrong.
///
/// # Errors
///
/// This function will return `AppError::DeserializationError` if there's an error deserializing the driver details from Redis.
///
pub async fn get_all_driver_last_locations(
    redis: &RedisConnectionPool,
    driver_ids: &[DriverId],
) -> Result<Vec<Option<DriverLastKnownLocation>>, AppError> {
    let driver_last_location_updates_keys = driver_ids
        .iter()
        .map(driver_details_key)
        .collect::<Vec<String>>();

    let driver_last_known_location = redis
        .mget_keys::<DriverAllDetails>(driver_last_location_updates_keys)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?
        .into_iter()
        .map(|opt_detail| opt_detail.map(|detail| detail.driver_last_known_location))
        .collect::<Vec<Option<DriverLastKnownLocation>>>();

    Ok(driver_last_known_location)
}

/// Push the driver location data to a non-persistent Redis store with specified expiration.
///
/// This function takes a map of geo entries representing driver locations and stores them in
/// a non-persistent Redis using the provided bucket expiry time. If there's no specified expiry for
/// individual entries, the entire bucket will expire after the provided time.
///
/// # Arguments
///
/// * `geo_entries` - A reference to a `FxHashMap` where the key represents the driver's identifier, city, bucket timestamp and merchantId
///   and the value is a vector of `GeoValue` indicating their respective geographical coordinates.
///
/// * `bucket_expiry` - A reference to an `i64` representing the time (in seconds) after which the
///   entire bucket of geo entries will expire in Redis.
///
/// * `redis` - A reference to the non-persistent Redis connection pool.
///
/// # Returns
///
/// A `Result` which is:
///
/// * `Ok(())` - Indicating the operation was successful.
/// * `Err(AppError)` - An error indicating what went wrong during the operation.
///
pub async fn push_drainer_driver_location(
    geo_entries: &DriversLocationMap,
    bucket_expiry: &i64,
    redis: &RedisConnectionPool,
) -> Result<(), AppError> {
    redis
        .mgeo_add_with_expiry(geo_entries, None, false, *bucket_expiry)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn remove_route_location(
    redis: &RedisConnectionPool,
    route_code: &str,
    vehicle_number: &str,
) -> Result<(), AppError> {
    redis
        .hdel(&driver_loc_based_on_route_key(route_code), vehicle_number)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub async fn set_route_location(
    redis: &RedisConnectionPool,
    route_code: &str,
    vehicle_number: &String,
    location: &Point,
    speed: &Option<SpeedInMeterPerSecond>,
    timestamp: &TimeStamp,
    ride_status: Option<RideStatus>,
    upcoming_stops: Option<Vec<UpcomingStop>>,
) -> Result<(), AppError> {
    let vehicle_tracking_info = VehicleTrackingInfo {
        schedule_relationship: None,
        start_time: None,
        trip_id: None,
        latitude: location.lat,
        longitude: location.lon,
        speed: *speed,
        timestamp: Some(*timestamp),
        ride_status,
        upcoming_stops,
    };
    redis
        .set_hash_fields_with_hashmap_expiry(
            &driver_loc_based_on_route_key(route_code),
            vec![(vehicle_number.to_owned(), vehicle_tracking_info)],
            86400,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn get_route_location(
    redis: &RedisConnectionPool,
    route_code: &str,
) -> Result<HashMap<String, VehicleTrackingInfo>, AppError> {
    redis
        .get_all_hash_fields(&driver_loc_based_on_route_key(route_code))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn get_route_location_by_vehicle_number(
    redis: &RedisConnectionPool,
    route_code: &str,
    vehicle_number: &str,
) -> Result<Option<VehicleTrackingInfo>, AppError> {
    redis
        .get_hash_field(&driver_loc_based_on_route_key(route_code), vehicle_number)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn get_trip_location(
    redis: &RedisConnectionPool,
    trip_code: &str,
) -> Result<HashMap<String, VehicleTrackingInfo>, AppError> {
    redis
        .get_all_hash_fields(&driver_loc_based_on_trip_key(trip_code))
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn cache_google_stop_duration(
    redis: &RedisConnectionPool,
    source_stop_code: String,
    destination_stop_code: String,
    duration: Seconds,
) -> Result<(), AppError> {
    let route_config_key = google_stop_duration_key(&source_stop_code, &destination_stop_code);
    redis
        .set_key_without_expiry(&route_config_key, duration)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn get_google_stop_duration(
    redis: &RedisConnectionPool,
    source_stop_code: String,
    destination_stop_code: String,
) -> Result<Option<Seconds>, AppError> {
    let route_config_key = google_stop_duration_key(&source_stop_code, &destination_stop_code);
    redis
        .get_key::<Seconds>(&route_config_key)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Add a driver to the special-location ZSET for the given bucket (when enable_special_location_bucketing).
/// Score = timestamp; member = driver_id (JSON string). Sets key expiry.
pub async fn add_driver_to_special_location_zset(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    bucket: u64,
    driver_id: &DriverId,
    timestamp: &TimeStamp,
    bucket_expiry: i64,
) -> Result<(), AppError> {
    let key = special_location_drivers_key(special_location_id, &bucket);
    let score = timestamp.inner().timestamp() as f64;
    let member =
        serde_json::to_string(&driver_id.0).map_err(|e| AppError::InternalError(e.to_string()))?;
    redis
        .zadd(
            &key,
            None,
            None,
            false,
            false,
            vec![(score, member.as_str())],
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    redis
        .set_expiry(&key, bucket_expiry)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

// --- Queue operations (airport/special zone FIFO queue, per vehicle type) ---

/// Tracking value stored in the driver queue tracking key.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DriverQueueTracking {
    pub special_location_id: String,
    pub vehicle_type: String,
}

/// Add a driver to the queue ZSET using ZADD NX (only if not already present).
/// This preserves original entry timestamp/position on subsequent pings.
pub async fn add_driver_to_queue(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    vehicle_type: &str,
    driver_id: &str,
    timestamp: f64,
    queue_expiry: i64,
) -> Result<(), AppError> {
    let key = special_location_queue_key(special_location_id, vehicle_type);
    let member =
        serde_json::to_string(driver_id).map_err(|e| AppError::InternalError(e.to_string()))?;
    redis
        .zadd(
            &key,
            Some(SetOptions::NX),
            None,
            false,
            false,
            vec![(timestamp, member.as_str())],
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    redis
        .set_expiry(&key, queue_expiry)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Remove a driver from the queue ZSET.
pub async fn remove_driver_from_queue(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    vehicle_type: &str,
    driver_id: &str,
) -> Result<(), AppError> {
    let key = special_location_queue_key(special_location_id, vehicle_type);
    let member =
        serde_json::to_string(driver_id).map_err(|e| AppError::InternalError(e.to_string()))?;
    let _: u64 = redis
        .writer_pool
        .zrem(&key, member)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(())
}

/// Get a driver's 0-based rank in the queue ZSET.
pub async fn get_driver_queue_position(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    vehicle_type: &str,
    driver_id: &str,
) -> Result<Option<u64>, AppError> {
    let key = special_location_queue_key(special_location_id, vehicle_type);
    let member =
        serde_json::to_string(driver_id).map_err(|e| AppError::InternalError(e.to_string()))?;
    let rank: Option<u64> = redis
        .reader_pool
        .zrank(&key, member)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(rank)
}

/// Get the total size of a queue ZSET.
pub async fn get_queue_size(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    vehicle_type: &str,
) -> Result<u64, AppError> {
    let key = special_location_queue_key(special_location_id, vehicle_type);
    redis
        .zcard(&key)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Set the tracking key for which queue a driver is currently in.
/// No TTL — only deleted explicitly on queue exit so we never lose track of
/// which queue a driver is in if location updates pause.
pub async fn set_driver_queue_tracking(
    redis: &RedisConnectionPool,
    merchant_id: &str,
    driver_id: &str,
    tracking: &DriverQueueTracking,
) -> Result<(), AppError> {
    let key = driver_queue_tracking_key(merchant_id, driver_id);
    let value =
        serde_json::to_string(tracking).map_err(|e| AppError::InternalError(e.to_string()))?;
    redis
        .set_key_without_expiry(&key, value)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Get the tracking info (special_location_id + vehicle_type) the driver is currently queued in.
pub async fn get_driver_queue_tracking(
    redis: &RedisConnectionPool,
    merchant_id: &str,
    driver_id: &str,
) -> Result<Option<DriverQueueTracking>, AppError> {
    let key = driver_queue_tracking_key(merchant_id, driver_id);
    let raw: Option<String> = redis
        .get_key::<String>(&key)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    match raw {
        Some(s) => {
            let tracking: DriverQueueTracking =
                serde_json::from_str(&s).map_err(|e| AppError::InternalError(e.to_string()))?;
            Ok(Some(tracking))
        }
        None => Ok(None),
    }
}

/// Delete the driver queue tracking key.
pub async fn delete_driver_queue_tracking(
    redis: &RedisConnectionPool,
    merchant_id: &str,
    driver_id: &str,
) -> Result<(), AppError> {
    let key = driver_queue_tracking_key(merchant_id, driver_id);
    redis
        .delete_key(&key)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Batch-fetch driver queue tracking for multiple (merchant_id, driver_id) pairs using MGET.
pub async fn batch_get_driver_queue_trackings(
    redis: &RedisConnectionPool,
    pairs: &[(&str, &str)],
) -> Result<Vec<Option<DriverQueueTracking>>, AppError> {
    if pairs.is_empty() {
        return Ok(vec![]);
    }
    let keys: Vec<String> = pairs
        .iter()
        .map(|(mid, did)| driver_queue_tracking_key(mid, did))
        .collect();
    redis
        .mget_keys::<DriverQueueTracking>(keys)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

pub async fn batch_check_drivers_on_ride(
    redis: &RedisConnectionPool,
    pairs: &[(&str, &str)],
) -> Result<Vec<bool>, AppError> {
    if pairs.is_empty() {
        return Ok(vec![]);
    }
    let keys: Vec<String> = pairs
        .iter()
        .map(|(mid, did)| {
            on_ride_details_key(&MerchantId(mid.to_string()), &DriverId(did.to_string()))
        })
        .collect();
    let results = redis
        .mget_keys::<RideDetails>(keys)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(results.iter().map(|r| r.is_some()).collect())
}

/// Add a driver to the queue ZSET using ZADD (without NX — overwrites existing score).
/// Used for manual queue insertion at a specific position.
pub async fn add_driver_to_queue_force(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    vehicle_type: &str,
    driver_id: &str,
    score: f64,
    queue_expiry: i64,
) -> Result<(), AppError> {
    let key = special_location_queue_key(special_location_id, vehicle_type);
    let member =
        serde_json::to_string(driver_id).map_err(|e| AppError::InternalError(e.to_string()))?;
    redis
        .zadd(
            &key,
            None, // no NX/XX option — always set
            None,
            false,
            false,
            vec![(score, member.as_str())],
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    redis
        .set_expiry(&key, queue_expiry)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
}

/// Get (member, score) pairs for a rank range in the queue ZSET (ZRANGE WITHSCORES).
pub async fn get_queue_scores_at_range(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    vehicle_type: &str,
    start: i64,
    end: i64,
) -> Result<Vec<(String, f64)>, AppError> {
    let key = special_location_queue_key(special_location_id, vehicle_type);
    let results: Vec<(String, f64)> = redis
        .reader_pool
        .zrange(&key, start, end, None, false, None, true)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    Ok(results)
}

/// Get all driver IDs in a special location by querying recent buckets.
pub async fn get_drivers_in_special_location(
    redis: &RedisConnectionPool,
    special_location_id: &str,
    current_bucket: &u64,
    nearby_bucket_threshold: &u64,
) -> Result<Vec<DriverId>, AppError> {
    let mut driver_ids = FxHashSet::default();
    for idx in 0..*nearby_bucket_threshold {
        let bucket = current_bucket.saturating_sub(idx);
        let key = special_location_drivers_key(special_location_id, &bucket);
        let members: Vec<String> = redis
            .zrange(&key, 0, -1, None, false, None, false)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        for s in members {
            if let Ok(id) = serde_json::from_str::<String>(&s) {
                driver_ids.insert(DriverId(id));
            }
        }
    }
    Ok(driver_ids.into_iter().collect())
}
