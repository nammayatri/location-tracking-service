use crate::common::types::*;
use crate::redis::{keys::*, types::*};
use actix_web::web::Data;
use fred::types::{GeoPosition, GeoUnit, MultipleGeoValues, RedisValue, SortOrder};
use futures::Future;
use shared::utils::logger::*;
use shared::{redis::types::RedisConnectionPool, tools::error::AppError};
use std::sync::Arc;

pub async fn get_drivers_within_radius(
    data: Data<AppState>,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    location: Point,
    radius: f64,
) -> Result<Vec<GeoSearch>, AppError> {
    let nearby_drivers = data
        .location_redis
        .geo_search(
            driver_loc_bucket_key(&merchant_id, &city, &vehicle, &bucket).as_str(),
            None,
            Some(GeoPosition::from((location.lon, location.lat))),
            Some((radius, GeoUnit::Kilometers)),
            None,
            Some(SortOrder::Asc),
            None,
            true,
            true,
            false,
        )
        .await?;

    let mut resp: Vec<GeoSearch> = Vec::new();

    for driver in nearby_drivers {
        match driver.member {
            RedisValue::String(driver_id) => {
                let pos = driver
                    .position
                    .expect("GeoPosition not found for geo search");
                let lon = pos.longitude;
                let lat = pos.latitude;

                resp.push(GeoSearch {
                    driver_id: driver_id.to_string(),
                    location: Point { lat: lat, lon: lon },
                });
            }
            _ => {
                info!("Invalid RedisValue variant");
            }
        }
    }

    return Ok(resp);
}

pub async fn push_drianer_values(
    data: Data<AppState>,
    key: String,
    multiple_geo_values: MultipleGeoValues,
) -> Result<(), AppError> {
    let _ = data
        .location_redis
        .geo_add(&key, multiple_geo_values, None, false)
        .await?;

    return Ok(());
}

pub async fn push_driver_location(
    data: Data<AppState>,
    key: String,
    multiple_geo_values: MultipleGeoValues,
) -> Result<(), AppError> {
    let _ = data
        .location_redis
        .geo_add(&key, multiple_geo_values, None, false)
        .await?;
    return Ok(());
}

pub async fn zrange(data: Data<AppState>, key: String) -> Result<Vec<String>, AppError> {
    let redis_val = data
        .location_redis
        .zrange(&key, 0, -1, None, false, None, false)
        .await?;

    match redis_val {
        RedisValue::Array(res) => {
            let resp = res
                .into_iter()
                .map(|x| match x {
                    RedisValue::String(y) => String::from_utf8(y.into_inner().to_vec()).unwrap(),
                    _ => String::from(""),
                })
                .collect::<Vec<String>>();

            return Ok(resp);
        }
        _ => {
            info!("Invalid RedisValue variant");
            return Err(AppError::InternalServerError);
        }
    }
}

pub async fn geopos(
    data: Data<AppState>,
    key: String,
    members: Vec<String>,
) -> Result<Vec<Point>, AppError> {
    let redis_val = data.location_redis.geopos(&key, members).await?;

    match redis_val {
        RedisValue::Array(res) => {
            let mut loc: Vec<Point> = Vec::new();

            for item in res {
                let item = item.as_geo_position().unwrap();
                match item {
                    Some(val) => {
                        loc.push(Point {
                            lon: val.longitude,
                            lat: val.latitude,
                        });
                    }
                    _ => {}
                }
            }

            return Ok(loc);
        }
        _ => {
            info!("Invalid RedisValue variant");
            return Err(AppError::InternalServerError);
        }
    }
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
        callback(args).await?;
        let _ = redis.delete_key(key).await;
    }

    Ok(())
}
