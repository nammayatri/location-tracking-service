/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use crate::environment::AppState;
use crate::redis::keys::*;
use actix_web::web::Data;
use chrono::{DateTime, Utc};
use fred::types::{GeoPosition, GeoUnit, GeoValue, MultipleGeoValues, RedisValue, SortOrder};
use futures::Future;
use shared::utils::logger::*;
use shared::{redis::types::RedisConnectionPool, tools::error::AppError};
use std::collections::HashSet;
use std::sync::Arc;

pub async fn set_ride_details(
    data: Data<AppState>,
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
        .map_err(|err| AppError::DeserializationError(err.to_string()))?;

    data.persistent_redis
        .set_with_expiry(
            &on_ride_details_key(merchant_id, driver_id),
            ride_details,
            data.redis_expiry,
        )
        .await?;

    Ok(())
}

pub async fn get_ride_details(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
) -> Result<Option<RideDetails>, AppError> {
    let ride_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_details_key(merchant_id, driver_id))
        .await?;

    match ride_details {
        Some(ride_details) => {
            let ride_details = serde_json::from_str::<RideDetails>(&ride_details)
                .map_err(|err| AppError::InternalError(err.to_string()))?;

            Ok(Some(ride_details))
        }
        None => Ok(None),
    }
}

pub async fn set_driver_details(
    data: Data<AppState>,
    ride_id: &RideId,
    driver_details: DriverDetails,
) -> Result<(), AppError> {
    let driver_details = serde_json::to_string(&driver_details)
        .map_err(|err| AppError::DeserializationError(err.to_string()))?;

    let _ = data
        .persistent_redis
        .set_with_expiry(
            &on_ride_driver_details_key(ride_id),
            driver_details,
            data.redis_expiry,
        )
        .await;

    Ok(())
}

pub async fn get_driver_details(
    data: Data<AppState>,
    wrapped_ride_id @ RideId(ride_id): &RideId,
) -> Result<DriverDetails, AppError> {
    let driver_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_driver_details_key(wrapped_ride_id))
        .await?;

    let driver_details = match driver_details {
        Some(driver_details) => driver_details,
        None => {
            return Err(AppError::InternalError(
                format!("Driver details not found for RideId : {ride_id}").to_string(),
            ))
        }
    };

    let driver_details = serde_json::from_str::<DriverDetails>(&driver_details)
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(driver_details)
}

#[allow(clippy::too_many_arguments)]
pub async fn get_drivers_within_radius(
    data: Data<AppState>,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    location: Point,
    Radius(radius): Radius,
) -> Result<Vec<DriverLocationPoint>, AppError> {
    let Latitude(lat) = location.lat;
    let Longitude(lon) = location.lon;
    let mut nearby_drivers = Vec::new();
    for bucket_idx in 0..data.nearby_bucket_threshold {
        let nearby_drivers_by_bucket = data
            .non_persistent_redis
            .geo_search(
                driver_loc_bucket_key(merchant_id, city, vehicle, &(bucket - bucket_idx)).as_str(),
                None,
                Some(GeoPosition::from((lon, lat))),
                Some((radius, GeoUnit::Meters)),
                None,
                Some(SortOrder::Asc),
                None,
                true,
                true,
                false,
            )
            .await?;
        nearby_drivers.extend(nearby_drivers_by_bucket);
    }

    info!("Get Nearby Drivers {:?}", nearby_drivers);

    let mut driver_ids: HashSet<DriverId> = HashSet::new();
    let mut resp: Vec<DriverLocationPoint> = Vec::new();

    for driver in nearby_drivers.iter() {
        if let (RedisValue::String(driver_id), Some(pos)) = (&driver.member, &driver.position) {
            let driver_id = DriverId(driver_id.to_string());
            if !(driver_ids.contains(&driver_id)) {
                driver_ids.insert(driver_id.clone());
                resp.push(DriverLocationPoint {
                    driver_id: driver_id,
                    location: Point {
                        lat: Latitude(pos.latitude),
                        lon: Longitude(pos.longitude),
                    },
                })
            }
        }
    }

    Ok(resp)
}

pub async fn push_drainer_driver_location(
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    geo_entries: &[(Latitude, Longitude, DriverId)],
    bucket_size: u64,
    nearby_bucket_threshold: u64,
    non_persistent_redis: Arc<RedisConnectionPool>,
) -> Result<(), AppError> {
    let geo_values: Vec<GeoValue> = geo_entries
        .iter()
        .map(
            |(Latitude(lat), Longitude(lon), DriverId(driver_id))| GeoValue {
                coordinates: GeoPosition {
                    latitude: *lat,
                    longitude: *lon,
                },
                member: driver_id.into(),
            },
        )
        .collect();
    let multiple_geo_values: MultipleGeoValues = geo_values.into();

    non_persistent_redis
        .geo_add_with_expiry(
            &driver_loc_bucket_key(merchant_id, city, vehicle, bucket),
            multiple_geo_values,
            None,
            false,
            bucket_size * nearby_bucket_threshold,
        )
        .await?;

    Ok(())
}

pub async fn get_driver_location(
    data: Data<AppState>,
    driver_id: &DriverId,
) -> Result<DriverLastKnownLocation, AppError> {
    let last_location_update = data
        .persistent_redis
        .get_key(&driver_details_key(driver_id))
        .await?;

    if let Some(val) = last_location_update {
        let driver_all_details = serde_json::from_str::<DriverAllDetails>(&val)
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        if let Some(last_known_location) = driver_all_details.driver_last_known_location {
            return Ok(last_known_location);
        }
    }

    Err(AppError::InternalError(
        "Failed to get_driver_location".to_string(),
    ))
}

pub async fn get_driver_last_location_update(
    data: Data<AppState>,
    driver_id: &DriverId,
) -> Result<TimeStamp, AppError> {
    let last_location_update = data
        .persistent_redis
        .get_key(&driver_details_key(driver_id))
        .await?;

    if let Some(val) = last_location_update {
        if let Ok(x) = serde_json::from_str::<DriverAllDetails>(&val) {
            if let Some(x) = x.driver_last_known_location {
                return Ok(TimeStamp(x.timestamp.with_timezone(&Utc)));
            }
        }
    }

    Err(AppError::InternalError(
        "Failed to get_driver_last_location_update".to_string(),
    ))
}

pub async fn set_driver_mode_details(
    data: Data<AppState>,
    driver_id: DriverId,
    driver_mode: DriverMode,
) -> Result<(), AppError> {
    let last_location_update = data
        .persistent_redis
        .get_key(&driver_details_key(&driver_id))
        .await?;
    let driver_all_details = if let Some(val) = last_location_update {
        let mut value = serde_json::from_str::<DriverAllDetails>(&val)
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        value.driver_mode = Some(driver_mode);
        value
    } else {
        DriverAllDetails {
            driver_mode: Some(driver_mode),
            driver_last_known_location: None,
        }
    };
    let value = serde_json::to_string(&driver_all_details)
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    data.persistent_redis
        .set_with_expiry(
            &driver_details_key(&driver_id),
            value,
            data.last_location_timstamp_expiry,
        )
        .await?;
    Ok(())
}

pub async fn set_driver_last_location_update(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    last_location: &Point,
    driver_mode: Option<DriverMode>,
) -> Result<(), AppError> {
    let last_location_update = data
        .persistent_redis
        .get_key(&driver_details_key(driver_id))
        .await?;

    let driver_all_details = if let Some(val) = last_location_update {
        let mut value = serde_json::from_str::<DriverAllDetails>(&val)
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        let driver_last_known_location: DriverLastKnownLocation = DriverLastKnownLocation {
            location: Point {
                lat: last_location.lat,
                lon: last_location.lon,
            },
            timestamp: Utc::now(),
            merchant_id: merchant_id.clone(),
        };
        value.driver_last_known_location = Some(driver_last_known_location);
        if driver_mode.is_some() {
            value.driver_mode = driver_mode;
        }
        value
    } else {
        DriverAllDetails {
            driver_mode,
            driver_last_known_location: Some(DriverLastKnownLocation {
                location: Point {
                    lat: last_location.lat,
                    lon: last_location.lon,
                },
                timestamp: Utc::now(),
                merchant_id: merchant_id.clone(),
            }),
        }
    };
    let value = serde_json::to_string(&driver_all_details)
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    data.persistent_redis
        .set_with_expiry(
            &driver_details_key(driver_id),
            value,
            data.last_location_timstamp_expiry,
        )
        .await?;

    Ok(())
}

pub async fn get_on_ride_driver_location_count(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
) -> Result<i64, AppError> {
    let driver_location_count = data
        .persistent_redis
        .llen(&on_ride_loc_key(merchant_id, driver_id))
        .await?;

    Ok(driver_location_count)
}

pub async fn push_on_ride_driver_locations(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    geo_entries: &Vec<Point>,
) -> Result<(), AppError> {
    let mut geo_points: Vec<String> = Vec::new();

    for entry in geo_entries {
        let value = serde_json::to_string(&entry)
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        geo_points.push(value);
    }

    let _ = data
        .persistent_redis
        .rpush(&on_ride_loc_key(merchant_id, driver_id), geo_points)
        .await?;

    Ok(())
}

pub async fn get_on_ride_driver_locations(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    len: i64,
) -> Result<Vec<Point>, AppError> {
    let output = data
        .persistent_redis
        .lpop(&on_ride_loc_key(merchant_id, driver_id), Some(len as usize))
        .await?;

    let mut geo_points: Vec<Point> = Vec::new();

    for point in output {
        let geo_point = serde_json::from_str::<Point>(&point)
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        geo_points.push(geo_point);
    }

    Ok(geo_points)
}

pub async fn set_driver_id(
    data: Data<AppState>,
    token: &Token,
    DriverId(driver_id): &DriverId,
) -> Result<(), AppError> {
    let _: () = data
        .persistent_redis
        .set_with_expiry(&set_driver_id_key(token), driver_id, data.auth_token_expiry)
        .await?;

    Ok(())
}

pub async fn get_driver_id(
    data: Data<AppState>,
    token: &Token,
) -> Result<Option<DriverId>, AppError> {
    let driver_id: Option<String> = data
        .persistent_redis
        .get_key(&set_driver_id_key(token))
        .await?;
    match driver_id {
        Some(driver_id) => Ok(Some(DriverId(driver_id))),
        None => Ok(None),
    }
}

pub async fn with_lock_redis<F, Args, Fut>(
    redis: Arc<RedisConnectionPool>,
    key: String,
    expiry: i64,
    callback: F,
    args: Args,
) -> Result<(), AppError>
where
    F: Fn(Args) -> Fut,
    Args: Send + 'static,
    Fut: Future<Output = ()>,
{
    let lock = redis.setnx_with_expiry(&key, true, expiry).await;

    if lock.is_ok() {
        info!("Got lock : {}", &key);
        let resp = callback(args).await;
        let _ = redis.delete_key(&key).await;
        info!("Released lock : {}", &key);
        return Ok(resp);
    }

    Err(AppError::HitsLimitExceeded)
}

pub async fn get_all_driver_last_locations(
    data: Data<AppState>,
    nearby_drivers: &Vec<DriverLocationPoint>,
) -> Result<Vec<Option<DateTime<Utc>>>, AppError> {
    let driver_last_location_updates_keys = nearby_drivers
        .iter()
        .map(|driver| driver_details_key(&driver.driver_id))
        .collect::<Vec<String>>();

    let driver_last_locs_ts = data
        .persistent_redis
        .mget_keys(driver_last_location_updates_keys)
        .await?;

    let driver_last_ts = driver_last_locs_ts
        .iter()
        .map(|driver_all_details| {
            let driver_all_details: Option<DriverAllDetails> =
                if let Some(driver_all_details) = driver_all_details {
                    let driver_all_details =
                        serde_json::from_str::<DriverAllDetails>(driver_all_details)
                            .map_err(|err| AppError::DeserializationError(err.to_string()));
                    match driver_all_details {
                        Ok(driver_all_details) => Some(driver_all_details),
                        Err(err) => {
                            error!("DriverAllDetails DeserializationError : {}", err);
                            None
                        }
                    }
                } else {
                    None
                };

            let driver_last_ts = driver_all_details
                .map(|driver_all_details| driver_all_details.driver_last_known_location)
                .flatten()
                .map(|driver_last_known_location| driver_last_known_location.timestamp);

            driver_last_ts
        })
        .collect::<Vec<Option<DateTime<Utc>>>>();

    return Ok(driver_last_ts);
}
