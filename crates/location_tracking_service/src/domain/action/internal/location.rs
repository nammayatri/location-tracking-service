use actix_web::web::Data;
use chrono::{TimeZone, Utc};
use shared::utils::logger::*;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use strum::IntoEnumIterator;

use crate::{
    common::{redis::*, types::*, utils::get_city},
    domain::types::internal::location::*,
    redis::commands::*,
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
            let resp = geo_search(
                data.clone(),
                driver_loc_bucket_key(
                    &request_body.clone().merchant_id,
                    &city,
                    &vehicle_type,
                    &current_bucket,
                ),
                request_body.clone().lon,
                request_body.clone().lat,
                request_body.clone().radius,
            )
            .await?;

            for item in resp {
                let timestamp = Utc.timestamp_opt((current_bucket * 60) as i64, 0).unwrap();
                let driver_location = DriverLocation {
                    driver_id: item.driver_id.to_string(),
                    lon: item.lon,
                    lat: item.lat,
                    coordinates_calculated_at: timestamp.clone(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp.clone(),
                    merchant_id: request_body.merchant_id.clone(),
                };
                resp_vec.push(driver_location);
            }
        }
    } else {
        let resp = geo_search(
            data.clone(),
            driver_loc_bucket_key(
                &request_body.clone().merchant_id,
                &city,
                &request_body.clone().vehicle_type.unwrap(),
                &current_bucket,
            ),
            request_body.clone().lon,
            request_body.clone().lat,
            request_body.clone().radius,
        )
        .await?;

        for item in resp {
            let timestamp = Utc.timestamp_opt((current_bucket * 60) as i64, 0).unwrap();
            let driver_location = DriverLocation {
                driver_id: item.driver_id.to_string(),
                lon: item.lon,
                lat: item.lat,
                coordinates_calculated_at: timestamp.clone(),
                created_at: timestamp.clone(),
                updated_at: timestamp.clone(),
                merchant_id: request_body.merchant_id.clone(),
            };
            resp_vec.push(driver_location);
        }
    }

    Ok(resp_vec)
}
