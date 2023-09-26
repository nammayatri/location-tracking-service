/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::web::Data;
use chrono::Utc;
use strum::IntoEnumIterator;

use crate::{
    common::{
        types::*,
        utils::{get_city, get_current_bucket},
    },
    domain::types::internal::location::*,
    environment::AppState,
    redis::commands::*,
};
use shared::{redis::types::RedisConnectionPool, tools::error::AppError, utils::logger::*};

#[allow(clippy::too_many_arguments)]
async fn search_nearby_drivers_with_vehicle(
    persistent_redis: &RedisConnectionPool,
    non_persistent_redis: &RedisConnectionPool,
    nearby_bucket_threshold: &u64,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    location: Point,
    radius: &Radius,
) -> Result<Vec<DriverLocation>, AppError> {
    let nearby_drivers = get_drivers_within_radius(
        non_persistent_redis,
        nearby_bucket_threshold,
        merchant_id,
        city,
        vehicle,
        bucket,
        location,
        radius,
    )
    .await?;

    let driver_ids: Vec<DriverId> = nearby_drivers
        .iter()
        .map(|driver| driver.driver_id.to_owned())
        .collect();

    let drivers_detail = get_all_driver_last_locations(persistent_redis, &driver_ids).await?;

    let resp = nearby_drivers
        .iter()
        .zip(drivers_detail.iter())
        .map(|(driver, driver_detail)| {
            let last_location_update_ts = driver_detail
                .as_ref()
                .and_then(|driver_all_details| {
                    driver_all_details.driver_last_known_location.as_ref()
                })
                .map(|driver_last_known_location| driver_last_known_location.timestamp)
                .unwrap_or(Utc::now());
            DriverLocation {
                driver_id: driver.driver_id.to_owned(),
                lat: driver.location.lat,
                lon: driver.location.lon,
                coordinates_calculated_at: last_location_update_ts,
                created_at: last_location_update_ts,
                updated_at: last_location_update_ts,
                merchant_id: merchant_id.to_owned(),
            }
        })
        .collect::<Vec<DriverLocation>>();

    Ok(resp)
}

pub async fn get_nearby_drivers(
    data: Data<AppState>,
    NearbyDriversRequest {
        lat,
        lon,
        vehicle_type,
        radius,
        merchant_id,
    }: NearbyDriversRequest,
) -> Result<NearbyDriverResponse, AppError> {
    let city = get_city(&lat, &lon, &data.polygon)?;

    let current_bucket = get_current_bucket(&data.bucket_size)?;

    match vehicle_type {
        None => {
            let mut resp: Vec<DriverLocation> = Vec::new();

            for vehicle in VehicleType::iter() {
                let nearby_drivers = search_nearby_drivers_with_vehicle(
                    &data.persistent_redis,
                    &data.non_persistent_redis,
                    &data.nearby_bucket_threshold,
                    &merchant_id,
                    &city,
                    &vehicle,
                    &current_bucket,
                    Point { lat, lon },
                    &radius,
                )
                .await;
                match nearby_drivers {
                    Ok(nearby_drivers) => {
                        resp.extend(nearby_drivers);
                    }
                    Err(err) => {
                        error!(tag="[Nearby Drivers For All Vehicle Types]", vehicle = %vehicle, "{:?}", err)
                    }
                }
            }

            Ok(resp)
        }
        Some(vehicle) => {
            let resp = search_nearby_drivers_with_vehicle(
                &data.persistent_redis,
                &data.non_persistent_redis,
                &data.nearby_bucket_threshold,
                &merchant_id,
                &city,
                &vehicle,
                &current_bucket,
                Point { lat, lon },
                &radius,
            )
            .await?;
            Ok(resp)
        }
    }
}

pub async fn get_drivers_location(
    data: Data<AppState>,
    driver_ids: Vec<DriverId>,
) -> Result<Vec<DriverLocation>, AppError> {
    let mut driver_locations = Vec::with_capacity(driver_ids.len());

    let drivers_detail = get_all_driver_last_locations(&data.persistent_redis, &driver_ids).await?;

    for (driver_id, driver_detail) in driver_ids.iter().zip(drivers_detail.iter()) {
        if let Some(driver_detail) = driver_detail {
            if let Some(driver_details) = &driver_detail.driver_last_known_location {
                let driver_location = DriverLocation {
                    driver_id: driver_id.to_owned(),
                    lat: driver_details.location.lat,
                    lon: driver_details.location.lon,
                    coordinates_calculated_at: driver_details.timestamp,
                    created_at: driver_details.timestamp,
                    updated_at: driver_details.timestamp,
                    merchant_id: driver_details.merchant_id.to_owned(),
                };
                driver_locations.push(driver_location);
            }
        }
    }

    Ok(driver_locations)
}
