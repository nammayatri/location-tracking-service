/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use crate::redis::keys::*;
use fred::types::{GeoPosition, GeoUnit, GeoValue, RedisValue, SortOrder};
use futures::Future;
use rustc_hash::{FxHashMap, FxHashSet};
use shared::utils::logger::*;
use shared::{redis::types::RedisConnectionPool, tools::error::AppError};

pub async fn set_ride_details(
    persistent_redis_pool: &RedisConnectionPool,
    redis_expiry: &u32,
    merchant_id: &MerchantId,
    driver_id: &DriverId,
    ride_id: RideId,
    ride_status: RideStatus,
) -> Result<(), AppError> {
    let ride_details = RideDetails {
        ride_id,
        ride_status,
    };
    let ride_details = serde_json::to_string(&ride_details)
        .map_err(|err| AppError::SerializationError(err.to_string()))?;

    persistent_redis_pool
        .set_key(
            &on_ride_details_key(merchant_id, driver_id),
            ride_details,
            *redis_expiry,
        )
        .await?;

    Ok(())
}

pub async fn get_ride_details(
    persistent_redis_pool: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
) -> Result<RideDetails, AppError> {
    let ride_details: Option<String> = persistent_redis_pool
        .get_key(&on_ride_details_key(merchant_id, driver_id))
        .await?;

    match ride_details {
        Some(ride_details) => {
            let ride_details = serde_json::from_str::<RideDetails>(&ride_details)
                .map_err(|err| AppError::DeserializationError(err.to_string()))?;

            Ok(ride_details)
        }
        None => Err(AppError::DriverRideDetailsNotFound),
    }
}

pub async fn ride_cleanup(
    persistent_redis_pool: &RedisConnectionPool,
    merchant_id: &MerchantId,
    driver_id: &DriverId,
    ride_id: &RideId,
) -> Result<(), AppError> {
    let _ = persistent_redis_pool
        .delete_keys(vec![
            &on_ride_details_key(merchant_id, driver_id),
            &on_ride_driver_details_key(ride_id),
            &on_ride_loc_key(merchant_id, driver_id),
        ])
        .await;

    Ok(())
}

pub async fn set_driver_details(
    persistent_redis_pool: &RedisConnectionPool,
    redis_expiry: &u32,
    ride_id: &RideId,
    driver_details: DriverDetails,
) -> Result<(), AppError> {
    let driver_details = serde_json::to_string(&driver_details)
        .map_err(|err| AppError::SerializationError(err.to_string()))?;

    let _ = persistent_redis_pool
        .set_key(
            &on_ride_driver_details_key(ride_id),
            driver_details,
            *redis_expiry,
        )
        .await;

    Ok(())
}

pub async fn get_driver_details(
    persistent_redis_pool: &RedisConnectionPool,
    ride_id: &RideId,
) -> Result<DriverDetails, AppError> {
    let driver_details: Option<String> = persistent_redis_pool
        .get_key(&on_ride_driver_details_key(ride_id))
        .await?;

    let driver_details = match driver_details {
        Some(driver_details) => driver_details,
        None => return Err(AppError::DriverRideDetailsNotFound),
    };

    let driver_details = serde_json::from_str::<DriverDetails>(&driver_details)
        .map_err(|err| AppError::DeserializationError(err.to_string()))?;

    Ok(driver_details)
}

#[allow(clippy::too_many_arguments)]
pub async fn get_drivers_within_radius(
    non_persistent_redis_pool: &RedisConnectionPool,
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

    let nearby_drivers = non_persistent_redis_pool
        .mgeo_search(
            bucket_keys,
            None,
            Some(GeoPosition::from((lon, lat))),
            Some((*radius, GeoUnit::Meters)),
            None,
            Some(SortOrder::Asc),
            None,
        )
        .await?;

    info!("Get Nearby Drivers {:?}", nearby_drivers);

    let mut driver_ids: FxHashSet<DriverId> = FxHashSet::default();
    let mut resp: Vec<DriverLocationPoint> = Vec::with_capacity(nearby_drivers.len());

    for driver in nearby_drivers.iter() {
        if let (RedisValue::String(driver_id), Some(pos)) = (&driver.member, &driver.position) {
            let driver_id = DriverId(driver_id.to_string());
            if !(driver_ids.contains(&driver_id)) {
                driver_ids.insert(driver_id.to_owned());
                resp.push(DriverLocationPoint {
                    driver_id,
                    location: Point {
                        lat: Latitude(pos.latitude),
                        lon: Longitude(pos.longitude),
                    },
                })
            }
        } else {
            error!(
                "GeoRadius Info not handled for mgeo_search for Driver : {:?}",
                driver
            )
        }
    }

    Ok(resp)
}

pub async fn get_driver_location(
    persistent_redis_pool: &RedisConnectionPool,
    driver_id: &DriverId,
) -> Result<DriverLastKnownLocation, AppError> {
    let last_location_update = persistent_redis_pool
        .get_key(&driver_details_key(driver_id))
        .await?;

    if let Some(val) = last_location_update {
        Ok(serde_json::from_str::<DriverAllDetails>(&val)
            .map_err(|err| AppError::DeserializationError(err.to_string()))?
            .driver_last_known_location)
    } else {
        Err(AppError::DriverLastKnownLocationNotFound)
    }
}

pub async fn set_driver_last_location_update(
    persistent_redis_pool: &RedisConnectionPool,
    last_location_timstamp_expiry: &u32,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    last_location_pt: &Point,
    last_location_ts: &TimeStamp,
) -> Result<DriverLastKnownLocation, AppError> {
    let last_known_location = DriverLastKnownLocation {
        location: Point {
            lat: last_location_pt.lat,
            lon: last_location_pt.lon,
        },
        timestamp: *last_location_ts,
        merchant_id: merchant_id.to_owned(),
    };

    let value = serde_json::to_string(&DriverAllDetails {
        driver_last_known_location: last_known_location.to_owned(),
    })
    .map_err(|err| AppError::DeserializationError(err.to_string()))?;

    persistent_redis_pool
        .set_key(
            &driver_details_key(driver_id),
            value,
            *last_location_timstamp_expiry,
        )
        .await?;

    Ok(last_known_location)
}

pub async fn push_on_ride_driver_locations(
    persistent_redis_pool: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    geo_entries: &[Point],
    rpush_expiry: &u32,
) -> Result<i64, AppError> {
    let geo_points: Vec<String> = geo_entries
        .iter()
        .map(serde_json::to_string)
        .filter_map(Result::ok)
        .collect();

    persistent_redis_pool
        .rpush_with_expiry(
            &on_ride_loc_key(merchant_id, driver_id),
            geo_points,
            *rpush_expiry,
        )
        .await
}

pub async fn get_on_ride_driver_locations_count(
    persistent_redis_pool: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
) -> Result<i64, AppError> {
    persistent_redis_pool
        .llen(&on_ride_loc_key(merchant_id, driver_id))
        .await
}

pub async fn get_on_ride_driver_locations(
    persistent_redis_pool: &RedisConnectionPool,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    len: i64,
) -> Result<Vec<Point>, AppError> {
    let output = persistent_redis_pool
        .lpop(&on_ride_loc_key(merchant_id, driver_id), Some(len as usize))
        .await?;

    let geo_points: Vec<Point> = output
        .iter()
        .map(|point| serde_json::from_str::<Point>(point))
        .filter_map(Result::ok)
        .collect();

    Ok(geo_points)
}

pub async fn set_driver_id(
    persistent_redis_pool: &RedisConnectionPool,
    auth_token_expiry: &u32,
    token: &Token,
    DriverId(driver_id): &DriverId,
) -> Result<(), AppError> {
    persistent_redis_pool
        .set_key(&set_driver_id_key(token), driver_id, *auth_token_expiry)
        .await?;

    Ok(())
}

pub async fn get_driver_id(
    persistent_redis_pool: &RedisConnectionPool,
    token: &Token,
) -> Result<Option<DriverId>, AppError> {
    let driver_id: Option<String> = persistent_redis_pool
        .get_key(&set_driver_id_key(token))
        .await?;
    match driver_id {
        Some(driver_id) => Ok(Some(DriverId(driver_id))),
        None => Ok(None),
    }
}

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
    let lock = redis.setnx_with_expiry(&key, true, expiry).await;

    if lock.is_ok() {
        info!("Got lock : {}", &key);
        let resp = callback(args).await;
        let _ = redis.delete_key(&key).await;
        info!("Released lock : {}", &key);
        return resp;
    }

    Err(AppError::HitsLimitExceeded(key))
}

pub async fn get_all_driver_last_locations(
    persistent_redis_pool: &RedisConnectionPool,
    driver_ids: &[DriverId],
) -> Result<Vec<Option<DriverLastKnownLocation>>, AppError> {
    let driver_last_location_updates_keys = driver_ids
        .iter()
        .map(driver_details_key)
        .collect::<Vec<String>>();

    let drivers_detail = persistent_redis_pool
        .mget_keys(driver_last_location_updates_keys)
        .await?;

    let drivers_detail = drivers_detail
        .iter()
        .map(|driver_all_details| {
            if let Some(driver_all_details) = driver_all_details {
                let driver_all_details =
                    serde_json::from_str::<DriverAllDetails>(driver_all_details)
                        .map_err(|err| AppError::DeserializationError(err.to_string()));
                match driver_all_details {
                    Ok(driver_all_details) => Some(driver_all_details.driver_last_known_location),
                    Err(err) => {
                        error!("DriverAllDetails DeserializationError : {}", err);
                        None
                    }
                }
            } else {
                None
            }
        })
        .collect::<Vec<Option<DriverLastKnownLocation>>>();

    Ok(drivers_detail)
}

#[allow(clippy::too_many_arguments)]
pub async fn push_drainer_driver_location(
    geo_entries: &FxHashMap<String, Vec<GeoValue>>,
    bucket_expiry: &i64,
    non_persistent_redis: &RedisConnectionPool,
) -> Result<(), AppError> {
    non_persistent_redis
        .mgeo_add_with_expiry(geo_entries, None, false, *bucket_expiry)
        .await?;

    Ok(())
}
