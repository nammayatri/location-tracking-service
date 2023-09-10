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
    city: &CityName,
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
            &on_ride_details_key(merchant_id, city, driver_id),
            ride_details,
            data.redis_expiry,
        )
        .await?;

    Ok(())
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

#[allow(clippy::too_many_arguments)]
pub async fn get_drivers_within_radius(
    data: Data<AppState>,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    location: Point,
    radius: f64,
    on_ride: bool,
) -> Result<Vec<DriverLocationPoint>, AppError> {
    let mut nearby_drivers = Vec::new();
    for bucket_idx in 0..data.nearby_bucket_threshold {
        let nearby_drivers_by_bucket = data
            .non_persistent_redis
            .geo_search(
                driver_loc_bucket_key(merchant_id, city, vehicle, &(bucket - bucket_idx)).as_str(),
                None,
                Some(GeoPosition::from((location.lon, location.lat))),
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

    let mut resp: Vec<DriverLocationPoint> = Vec::new();

    let driver_ids_with_geopostions = nearby_drivers
        .iter()
        .map(|driver| match &driver.member {
            RedisValue::String(driver_id) => {
                let pos = driver
                    .position
                    .clone()
                    .expect("GeoPosition not found for geo search");
                (driver_id.to_string(), pos)
            }
            _ => ("".to_string(), GeoPosition::from((0.0, 0.0))),
        })
        .collect::<Vec<(String, GeoPosition)>>();

    let driver_ids = driver_ids_with_geopostions
        .iter()
        .map(|(driver_id, _)| driver_id.to_string())
        .collect::<Vec<String>>();

    let drivers_ride_status =
        get_all_drivers_ride_details(data.clone(), &driver_ids, merchant_id, city).await?;

    let drivers_ride_details = driver_ids_with_geopostions
        .iter()
        .zip(drivers_ride_status.iter())
        .map(|(driver_id, ride_status)| {
            let driver_status = DriversRideStatus {
                driver_id: driver_id.0.clone(),
                ride_status: ride_status.clone(),
                location: Point {
                    lat: driver_id.1.latitude,
                    lon: driver_id.1.longitude,
                },
            };
            driver_status
        })
        .collect::<Vec<DriversRideStatus>>();

    if on_ride {
        for driver_ride_detail in drivers_ride_details {
            let ride_status = driver_ride_detail.ride_status;
            let driver_id = driver_ride_detail.driver_id;
            let lat = driver_ride_detail.location.lat;
            let lon = driver_ride_detail.location.lon;

            if let Some(ride_status) = ride_status {
                if (ride_status == RideStatus::INPROGRESS) || (ride_status == RideStatus::NEW) {
                    resp.push(DriverLocationPoint {
                        driver_id: driver_id.to_string(),
                        location: Point { lat, lon },
                    });
                }
            }
        }
    } else {
        for driver_ride_detail in drivers_ride_details {
            let ride_status = driver_ride_detail.ride_status;
            let driver_id = driver_ride_detail.driver_id;
            let lat = driver_ride_detail.location.lat;
            let lon = driver_ride_detail.location.lon;

            if let Some(ride_status) = ride_status {
                if (ride_status != RideStatus::INPROGRESS) && (ride_status != RideStatus::NEW) {
                    resp.push(DriverLocationPoint {
                        driver_id: driver_id.to_string(),
                        location: Point { lat, lon },
                    });
                }
            } else {
                resp.push(DriverLocationPoint {
                    driver_id: driver_id.to_string(),
                    location: Point { lat, lon },
                });
            }
        }
    }

    let mut driver_ids: HashSet<String> = HashSet::new();
    let mut result: Vec<DriverLocationPoint> = Vec::new();

    for item in resp {
        if !(driver_ids.contains(&item.driver_id)) {
            driver_ids.insert((item.driver_id).clone());
            result.push(item);
        }
    }

    Ok(result)
}

pub async fn push_drainer_driver_location(
    data: Data<AppState>,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    geo_entries: &[(Latitude, Longitude, DriverId)],
) -> Result<(), AppError> {
    let geo_values: Vec<GeoValue> = geo_entries
        .iter()
        .map(|(lat, lon, driver_id)| GeoValue {
            coordinates: GeoPosition {
                latitude: *lat,
                longitude: *lon,
            },
            member: driver_id.into(),
        })
        .collect();
    let multiple_geo_values: MultipleGeoValues = geo_values.into();

    data.non_persistent_redis
        .geo_add_with_expiry(
            &driver_loc_bucket_key(merchant_id, city, vehicle, bucket),
            multiple_geo_values,
            None,
            false,
            data.bucket_size * data.nearby_bucket_threshold,
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
) -> Result<DateTime<Utc>, AppError> {
    let last_location_update = data
        .persistent_redis
        .get_key(&driver_details_key(driver_id))
        .await?;

    if let Some(val) = last_location_update {
        if let Ok(x) = serde_json::from_str::<DriverAllDetails>(&val) {
            if let Some(x) = x.driver_last_known_location {
                return Ok(x.timestamp.with_timezone(&Utc));
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
            merchant_id: merchant_id.to_string(),
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
                merchant_id: merchant_id.to_string(),
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

pub async fn get_driver_ride_status(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
) -> Result<Option<RideStatus>, AppError> {
    let ride_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_details_key(merchant_id, city, driver_id))
        .await?;

    match ride_details {
        Some(ride_details) => {
            let ride_details = serde_json::from_str::<RideDetails>(&ride_details)
                .map_err(|err| AppError::InternalError(err.to_string()))?;

            Ok(Some(ride_details.ride_status))
        }
        None => Ok(None),
    }
}

pub async fn get_all_drivers_ride_details(
    data: Data<AppState>,
    driver_id: &Vec<DriverId>,
    merchant_id: &MerchantId,
    city: &CityName,
) -> Result<Vec<Option<RideStatus>>, AppError> {
    let on_ride_details_keys = driver_id
        .iter()
        .map(|driver_id| on_ride_details_key(merchant_id, city, driver_id))
        .collect::<Vec<String>>();

    let ride_details: Vec<Option<String>> = data
        .persistent_redis
        .mget_keys(on_ride_details_keys)
        .await?;

    let ride_status: Vec<Option<RideStatus>> = ride_details
        .into_iter()
        .map(|ride_details| {
            if let Some(ride_details) = ride_details {
                let ride_details = serde_json::from_str::<RideDetails>(&ride_details)
                    .map_err(|err| AppError::DeserializationError(err.to_string()));
                match ride_details {
                    Ok(ride_details) => Some(ride_details.ride_status),
                    Err(err) => {
                        error!("RideDetails DeserializationError : {}", err);
                        None
                    }
                }
            } else {
                None
            }
        })
        .collect();

    Ok(ride_status)
}

pub async fn get_driver_ride_details(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
) -> Result<Option<RideDetails>, AppError> {
    let ride_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_details_key(merchant_id, city, driver_id))
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

pub async fn get_driver_details(
    data: Data<AppState>,
    ride_id: &RideId,
) -> Result<DriverDetails, AppError> {
    let driver_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_driver_details_key(ride_id))
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

pub async fn push_on_ride_driver_location(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
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
        .rpush(&on_ride_loc_key(merchant_id, city, driver_id), geo_points)
        .await?;

    Ok(())
}

pub async fn get_on_ride_driver_location_count(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
) -> Result<i64, AppError> {
    let driver_location_count = data
        .persistent_redis
        .llen(&on_ride_loc_key(merchant_id, city, driver_id))
        .await?;

    Ok(driver_location_count)
}

pub async fn get_on_ride_driver_locations(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
    len: i64,
) -> Result<Vec<Point>, AppError> {
    let output = data
        .persistent_redis
        .lpop(
            &on_ride_loc_key(merchant_id, city, driver_id),
            Some(len as usize),
        )
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
    token: &String,
    driver_id: &DriverId,
) -> Result<(), AppError> {
    let _: () = data
        .persistent_redis
        .set_with_expiry(&set_driver_id_key(token), driver_id, data.auth_token_expiry)
        .await?;

    Ok(())
}

pub async fn get_driver_id(
    data: Data<AppState>,
    token: &String,
) -> Result<Option<String>, AppError> {
    let driver_id: Option<String> = data
        .persistent_redis
        .get_key(&set_driver_id_key(token))
        .await?;
    Ok(driver_id)
}

pub async fn with_lock_redis<F, Args, Fut>(
    redis: Arc<RedisConnectionPool>,
    key: String,
    expiry: i64,
    callback: F,
    args: Args,
) -> ()
where
    F: Fn(Args) -> Fut,
    Args: Send + 'static,
    Fut: Future<Output = ()>,
{
    let lock = redis.setnx_with_expiry(&key, true, expiry).await;

    if lock.is_ok() {
        let resp = callback(args).await;
        let _ = redis.delete_key(&key).await;
        resp
    }
}

pub async fn get_all_driver_last_locations(
    data: Data<AppState>,
    nearby_drivers: &Vec<DriverLocationPoint>,
) -> Result<Vec<Option<DateTime<Utc>>>, AppError> {
    let driver_last_location_updates_keys = nearby_drivers
        .iter()
        .map(|driver| driver_details_key(&driver.driver_id.to_string()))
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
