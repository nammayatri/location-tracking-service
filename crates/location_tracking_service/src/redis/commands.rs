/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;
use crate::domain::types::ui::location::UpdateDriverLocationRequest;
use crate::redis::keys::*;
use actix_web::web::Data;
use chrono::{DateTime, Utc};
use fred::types::{GeoPosition, GeoUnit, GeoValue, MultipleGeoValues, RedisValue, SortOrder};
use futures::Future;
use shared::utils::{logger::*, prometheus};
use shared::{redis::types::RedisConnectionPool, tools::error::AppError};
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
    let ride_details = serde_json::to_string(&ride_details).unwrap();

    data.persistent_redis
        .set_with_expiry(
            &on_ride_details_key(&merchant_id, &city, &driver_id),
            ride_details,
            data.redis_expiry,
        )
        .await?;

    Ok(())
}

pub async fn set_driver_details(
    data: Data<AppState>,
    merchant_id: MerchantId,
    city: CityName,
    driver_id: DriverId,
    ride_id: RideId,
) -> Result<(), AppError> {
    let driver_details = DriverDetails {
        driver_id,
        merchant_id,
        city,
    };
    let driver_details = serde_json::to_string(&driver_details).unwrap();

    data.persistent_redis
        .set_with_expiry(
            &on_ride_driver_details_key(&ride_id),
            driver_details,
            data.redis_expiry,
        )
        .await;

    Ok(())
}

pub async fn set_driver_mode_details(
    data: Data<AppState>,
    driver_id: DriverId,
    driver_mode: DriverMode,
) -> Result<(), AppError> {
    let value = DriverModeDetails {
        driver_id: driver_id.clone(),
        driver_mode,
    };
    let value =
        serde_json::to_string(&value).map_err(|err| AppError::InternalError(err.to_string()))?;

    data.persistent_redis
        .set_with_expiry(&driver_details_key(&driver_id), value, data.redis_expiry)
        .await
}

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
    let nearby_drivers = data
        .non_persistent_redis
        .geo_search(
            driver_loc_bucket_key(&merchant_id, &city, &vehicle, &bucket, on_ride).as_str(),
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

    let mut resp: Vec<DriverLocationPoint> = Vec::new();

    for driver in nearby_drivers {
        match driver.member {
            RedisValue::String(driver_id) => {
                let pos = driver
                    .position
                    .expect("GeoPosition not found for geo search");
                let lon = pos.longitude;
                let lat = pos.latitude;

                resp.push(DriverLocationPoint {
                    driver_id: driver_id.to_string(),
                    location: Point { lat: lat, lon: lon },
                });
            }
            _ => {
                info!(tag = "[Redis]", "Invalid RedisValue variant");
            }
        }
    }

    return Ok(resp);
}

pub async fn push_drainer_driver_location(
    data: Data<AppState>,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    on_ride: bool,
    bucket: &u64,
    geo_entries: &Vec<(Latitude, Longitude, DriverId)>,
) -> Result<(), AppError> {
    let geo_values: Vec<GeoValue> = geo_entries
        .to_owned()
        .into_iter()
        .map(|(lat, lon, driver_id)| {
            prometheus::QUEUE_GUAGE.dec();
            GeoValue {
                coordinates: GeoPosition {
                    latitude: lat,
                    longitude: lon,
                },
                member: driver_id.into(),
            }
        })
        .collect();
    let multiple_geo_values: MultipleGeoValues = geo_values.into();

    let _ = data
        .non_persistent_redis
        .geo_add(
            &driver_loc_bucket_key(merchant_id, city, vehicle, bucket, on_ride),
            multiple_geo_values,
            None,
            false,
        )
        .await?;

    return Ok(());
}

pub async fn get_driver_location(
    data: Data<AppState>,
    driver_id: &DriverId,
) -> Result<DriverLastKnownLocation, AppError> {
    let last_location_update = data
        .persistent_redis
        .get_key(&driver_last_loc_key(&driver_id))
        .await?;

    if let Some(val) = last_location_update {
        let last_known_location = serde_json::from_str::<DriverLastKnownLocation>(&val)
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        return Ok(last_known_location);
    }

    return Err(AppError::InternalError(
        "Failed to get_driver_location".to_string(),
    ));
}

pub async fn get_and_set_driver_last_location_update(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    last_location: &UpdateDriverLocationRequest,
) -> Result<DateTime<Utc>, AppError> {
    let last_location_update = data
        .persistent_redis
        .get_key(&driver_last_loc_key(&driver_id))
        .await?;

    let value: DriverLastKnownLocation = DriverLastKnownLocation {
        location: Point {
            lat: last_location.pt.lat,
            lon: last_location.pt.lon,
        },
        timestamp: Utc::now(),
        merchant_id: merchant_id.to_string(),
    };

    let value =
        serde_json::to_string(&value).map_err(|err| AppError::InternalError(err.to_string()))?;

    let _ = data
        .persistent_redis
        .set_with_expiry(
            &driver_last_loc_key(&driver_id),
            value,
            data.last_location_timstamp_expiry
                .try_into()
                .expect("Failed to parse last_location_timstamp_expiry"),
        )
        .await?;

    if let Some(val) = last_location_update {
        if let Ok(x) = serde_json::from_str::<DriverLastKnownLocation>(&val) {
            return Ok(x.timestamp.with_timezone(&Utc));
        }
    }

    return Err(AppError::InternalError(
        "Failed to get_and_set_driver_last_location_update".to_string(),
    ));
}

pub async fn get_driver_ride_status(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
) -> Result<Option<RideStatus>, AppError> {
    let ride_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_details_key(&merchant_id, &city, &driver_id))
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

pub async fn get_driver_ride_details(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
) -> Result<RideDetails, AppError> {
    let ride_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_details_key(&merchant_id, &city, &driver_id))
        .await?;

    let ride_details = match ride_details {
        Some(ride_details) => ride_details,
        None => {
            return Err(AppError::InternalError(
                "Driver ride details not found".to_string(),
            ))
        }
    };

    let ride_details = serde_json::from_str::<RideDetails>(&ride_details)
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(ride_details)
}

pub async fn get_driver_details(
    data: Data<AppState>,
    ride_id: &RideId,
) -> Result<DriverDetails, AppError> {
    let driver_details: Option<String> = data
        .persistent_redis
        .get_key(&on_ride_driver_details_key(&ride_id))
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
        .rpush(
            &on_ride_loc_key(&merchant_id, &city, &driver_id),
            geo_points,
        )
        .await?;

    return Ok(());
}

pub async fn get_on_ride_driver_location_count(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
) -> Result<i64, AppError> {
    let driver_location_count = data
        .persistent_redis
        .llen(&on_ride_loc_key(&merchant_id, &city, &driver_id))
        .await?;

    Ok(driver_location_count)
}

pub async fn get_on_ride_driver_locations(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
    pop_length: usize,
) -> Result<Vec<Point>, AppError> {
    let output = data
        .persistent_redis
        .rpop(
            &on_ride_loc_key(&merchant_id, &city, &driver_id),
            Some(pop_length),
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

pub async fn with_lock_redis<F, Args, Fut>(
    redis: Arc<RedisConnectionPool>,
    key: &str,
    expiry: i64,
    callback: F,
    args: Args,
) -> Result<(), AppError>
where
    F: Fn(Args) -> Fut,
    Args: Send + 'static,
    Fut: Future<Output = Result<(), AppError>>,
{
    let lock = redis.setnx_with_expiry(key, true, expiry).await;

    if let Ok(_) = lock {
        let resp = callback(args).await;
        let _ = redis.delete_key(key).await;
        resp?
    } else {
        return Err(AppError::HitsLimitExceeded);
    }
    Ok(())
}
