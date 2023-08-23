use crate::common::types::*;
use crate::redis::types::*;
use actix_web::web::Data;
use fred::types::{GeoPosition, GeoUnit, MultipleGeoValues, RedisValue, SortOrder};
use shared::tools::error::AppError;
use shared::utils::logger::*;

pub async fn geo_search(
    data: Data<AppState>,
    key: String,
    lon: f64,
    lat: f64,
    radius: f64,
) -> Result<Vec<GeoSearch>, AppError> {
    let resp = data
        .location_redis
        .geo_search(
            key.as_str(),
            None,
            Some(GeoPosition::from((lon, lat))),
            Some((radius, GeoUnit::Kilometers)),
            None,
            Some(SortOrder::Asc),
            None,
            true,
            true,
            false,
        )
        .await?;

    let mut resp_vec: Vec<GeoSearch> = Vec::new();

    for item in resp.unwrap() {
        match item.member {
            RedisValue::String(driver_id) => {
                let pos = item.position.unwrap();
                let lon = pos.longitude;
                let lat = pos.latitude;

                resp_vec.push(GeoSearch {
                    driver_id: driver_id.to_string(),
                    lon: lon,
                    lat: lat,
                });
            }
            _ => {
                info!("Invalid RedisValue variant");
            }
        }
    }

    return Ok(resp_vec);
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
