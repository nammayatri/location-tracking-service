/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::{types::*, utils::cat_maybes};
use crate::redis::keys::*;
use crate::tools::error::AppError;
use fred::types::{GeoPosition, GeoUnit, SortOrder};
use futures::Future;
use rustc_hash::FxHashSet;
use shared::redis::types::{RedisConnectionPool, Ttl};
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
/// * `vehicle_number` - Driver's vehicle number.
/// * `ride_start_otp` - The otp to start the ride.
/// * `estimated_pickup_distance` - The estimated pickup distance from driver's current location to customer's pickup location.
///
/// # Returns
/// * A Result indicating the success or failure of the operation.
#[allow(clippy::too_many_arguments)]
pub async fn set_ride_details(
    redis: &RedisConnectionPool,
    redis_expiry: &u32,
    merchant_id: &MerchantId,
    driver_id: &DriverId,
    ride_id: RideId,
    ride_status: RideStatus,
    vehicle_number: Option<String>,
    ride_start_otp: Option<u32>,
    estimated_pickup_distance: Option<Meters>,
) -> Result<(), AppError> {
    let ride_details = RideDetails {
        ride_id,
        ride_status,
        vehicle_number,
        ride_start_otp,
        estimated_pickup_distance,
    };
    redis
        .set_key(
            &on_ride_details_key(merchant_id, driver_id),
            ride_details,
            *redis_expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

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
) -> Result<(), AppError> {
    redis
        .delete_keys(vec![
            &on_ride_details_key(merchant_id, driver_id),
            &on_ride_driver_details_key(ride_id),
            &on_ride_loc_key(merchant_id, driver_id),
        ])
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))
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
pub async fn set_driver_details(
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

    let nearby_drivers = redis
        .mgeo_search(
            bucket_keys,
            GeoPosition::from((lon, lat)),
            (*radius, GeoUnit::Meters),
            SortOrder::Asc,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    let nearby_drivers: Vec<(DriverId, Point)> = cat_maybes(nearby_drivers)
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
) -> Result<DriverLastKnownLocation, AppError> {
    let last_known_location = DriverLastKnownLocation {
        location: Point {
            lat: last_location_pt.lat,
            lon: last_location_pt.lon,
        },
        timestamp: *last_location_ts,
        merchant_id: merchant_id.to_owned(),
    };

    let value = DriverAllDetails {
        driver_last_known_location: last_known_location.to_owned(),
        blocked_till: blocked_till.to_owned(),
        stop_detection,
        ride_status: ride_status.to_owned(),
        // travelled_distance: Some(travelled_distance),
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
    geo_entries: Vec<Point>,
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
) -> Result<Vec<Point>, AppError> {
    redis
        .lpop::<Point>(&on_ride_loc_key(merchant_id, driver_id), Some(len as usize))
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
) -> Result<Vec<Point>, AppError> {
    redis
        .lrange::<Point>(&on_ride_loc_key(merchant_id, driver_id), 0, len)
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
pub async fn with_lock_redis<F, Args, Fut>(
    redis: &RedisConnectionPool,
    key: String,
    expiry: i64,
    callback: F,
    args: Args,
) -> Result<(), AppError>
where
    F: Fn(Args) -> Fut,
    Args: Send + 'static,
    Fut: Future<Output = Result<(), AppError>>,
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
                    redis
                        .delete_key(&key)
                        .await
                        .map_err(|err| AppError::InternalError(err.to_string()))
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
