use std::time::{Duration, SystemTime, UNIX_EPOCH};

use actix_web::web::Data;
use chrono::TimeZone;
use chrono::Utc;
use fred::types::{GeoPosition, GeoUnit, RedisValue, SortOrder};
use shared::utils::logger::*;
use strum::IntoEnumIterator;

use crate::{
    common::{redis::*, types::*, utils::get_city},
    domain::types::internal::location::*,
};
use shared::tools::error::AppError;

pub async fn get_nearby_drivers(
    data: Data<AppState>,
    request_body: NearbyDriversRequest,
) -> Result<NearbyDriverResponse, AppError> {
    let location_expiry_in_seconds = data.bucket_expiry;
    let current_bucket =
        Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / location_expiry_in_seconds;
    info!("current bucket: {:?}", current_bucket);

    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    let mut resp_vec: Vec<DriverLocation> = Vec::new();

    if request_body.vehicle_type == Option::None {
        for vehicle_type in VehicleType::iter() {
            info!("vechile type{:?}", vehicle_type);
            let resp = data
                .location_redis
                .geo_search(
                    &driver_loc_bucket_key(
                        &request_body.merchant_id,
                        &city,
                        &vehicle_type.to_string(),
                        &current_bucket,
                    ),
                    None,
                    Some(GeoPosition::from((request_body.lon, request_body.lat))),
                    Some((request_body.radius, GeoUnit::Kilometers)),
                    None,
                    Some(SortOrder::Asc),
                    None,
                    true,
                    true,
                    false,
                )
                .await?;

            for item in resp.unwrap() {
                if let RedisValue::String(driver_id) = item.member {
                    let pos = item.position.unwrap();
                    let timestamp = Utc.timestamp_opt((current_bucket * 60) as i64, 0).unwrap();
                    let driver_location = DriverLocation {
                        driver_id: driver_id.to_string(),
                        lon: pos.longitude,
                        lat: pos.latitude,
                        coordinates_calculated_at: timestamp.clone(),
                        created_at: timestamp.clone(),
                        updated_at: timestamp.clone(),
                        merchant_id: request_body.merchant_id.clone(),
                    };
                    resp_vec.push(driver_location);
                }
            }
        }
    } else {
        let resp = data
            .location_redis
            .geo_search(
                &driver_loc_bucket_key(
                    &request_body.merchant_id,
                    &city,
                    &request_body.vehicle_type.unwrap().to_string(),
                    &current_bucket,
                ),
                None,
                Some(GeoPosition::from((request_body.lon, request_body.lat))),
                Some((request_body.radius, GeoUnit::Kilometers)),
                None,
                Some(SortOrder::Asc),
                None,
                true,
                true,
                false,
            )
            .await?;

        for item in resp.unwrap() {
            if let RedisValue::String(driver_id) = item.member {
                let pos = item.position.unwrap();
                let timestamp = Utc.timestamp_opt((current_bucket * 60) as i64, 0).unwrap();
                let driver_location = DriverLocation {
                    driver_id: driver_id.to_string(),
                    lon: pos.longitude,
                    lat: pos.latitude,
                    coordinates_calculated_at: timestamp.clone(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp.clone(),
                    merchant_id: request_body.merchant_id.clone(),
                };
                resp_vec.push(driver_location);
            }
        }
    }

    Ok(resp_vec)
}
