use actix_web::web::Data;
use chrono::{LocalResult, TimeZone, Utc};
use strum::IntoEnumIterator;

use crate::{
    common::{
        types::*,
        utils::{get_city, get_current_bucket},
    },
    domain::types::internal::location::*,
    redis::commands::*,
};
use shared::tools::error::AppError;

async fn search_nearby_drivers_with_vehicle(
    data: Data<AppState>,
    merchant_id: MerchantId,
    city: CityName,
    vehicle: VehicleType,
    bucket: u64,
    location: Point,
    radius: Radius,
) -> Result<Vec<DriverLocation>, AppError> {
    let nearby_drivers = get_drivers_within_radius(
        data,
        &merchant_id,
        &city,
        &vehicle,
        &bucket,
        location,
        radius,
    )
    .await?;

    let timestamp = Utc.timestamp_opt((bucket * 60) as i64, 0);

    let timestamp = if let LocalResult::Single(timestamp) = timestamp {
        timestamp
    } else {
        Utc::now()
    };

    let mut resp: Vec<DriverLocation> = Vec::new();

    for driver in nearby_drivers {
        let driver_location = DriverLocation {
            driver_id: driver.driver_id.to_string(),
            lat: driver.location.lat,
            lon: driver.location.lon,
            coordinates_calculated_at: timestamp.clone(),
            created_at: timestamp.clone(),
            updated_at: timestamp.clone(),
            merchant_id: merchant_id.clone(),
        };
        resp.push(driver_location);
    }

    Ok(resp)
}

pub async fn get_nearby_drivers(
    data: Data<AppState>,
    request_body: NearbyDriversRequest,
) -> Result<NearbyDriverResponse, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    let current_bucket = get_current_bucket(data.bucket_expiry)?;

    match request_body.clone().vehicle_type {
        None => {
            let mut resp: Vec<DriverLocation> = Vec::new();

            for vehicle in VehicleType::iter() {
                let nearby_drivers = search_nearby_drivers_with_vehicle(
                    data.clone(),
                    request_body.clone().merchant_id,
                    city.clone(),
                    vehicle,
                    current_bucket,
                    Point {
                        lat: request_body.clone().lat,
                        lon: request_body.clone().lon,
                    },
                    request_body.clone().radius,
                )
                .await?;
                resp.extend(nearby_drivers.iter().cloned());
            }

            Ok(resp)
        }
        Some(vehicle) => {
            let resp = search_nearby_drivers_with_vehicle(
                data.clone(),
                request_body.clone().merchant_id,
                city,
                vehicle,
                current_bucket,
                Point {
                    lat: request_body.clone().lat,
                    lon: request_body.clone().lon,
                },
                request_body.clone().radius,
            )
            .await?;
            Ok(resp)
        }
    }
}
