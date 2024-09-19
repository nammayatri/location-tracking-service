/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::tools::error::AppError;
use crate::{
    common::{
        types::*,
        utils::{get_bucket_from_timestamp, get_city},
    },
    domain::types::internal::location::*,
    environment::AppState,
    redis::commands::*,
    tools::prometheus::MEASURE_DURATION,
};
use actix_web::web::Data;
use chrono::Utc;
use shared::measure_latency_duration;
use shared::redis::types::RedisConnectionPool;
use shared::tools::logger::*;
use strum::IntoEnumIterator;

#[macros::measure_duration]
#[allow(clippy::too_many_arguments)]
async fn search_nearby_drivers_with_vehicle(
    redis: &RedisConnectionPool,
    nearby_bucket_threshold: &u64,
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle: &VehicleType,
    bucket: &u64,
    location: Point,
    radius: &Radius,
) -> Result<Vec<DriverLocation>, AppError> {
    let nearby_drivers = get_drivers_within_radius(
        redis,
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

    let driver_last_known_location = get_all_driver_last_locations(redis, &driver_ids).await?;

    let resp = nearby_drivers
        .iter()
        .zip(driver_last_known_location.iter())
        .map(|(driver, driver_last_known_location)| {
            let last_location_update_ts = driver_last_known_location
                .as_ref()
                .map(|driver_last_known_location| driver_last_known_location.timestamp)
                .unwrap_or(TimeStamp(Utc::now()));
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

#[macros::measure_duration]
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

    let current_bucket = get_bucket_from_timestamp(&data.bucket_size, TimeStamp(Utc::now()));

    match vehicle_type {
        None => {
            let mut resp: Vec<DriverLocation> = Vec::new();

            for vehicle in VehicleType::iter() {
                let nearby_drivers = search_nearby_drivers_with_vehicle(
                    &data.redis,
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
        Some(vehicles) => {
            let mut resp: Vec<DriverLocation> = Vec::new();
            for vehicle in vehicles {
                let nearby_drivers = search_nearby_drivers_with_vehicle(
                    &data.redis,
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
                        error!(tag="[Nearby Drivers For Specific Vehicle Types]", vehicle = %vehicle, "{:?}", err)
                    }
                }
            }
            Ok(resp)
        }
    }
}

#[macros::measure_duration]
pub async fn get_drivers_location(
    data: Data<AppState>,
    driver_ids: Vec<DriverId>,
) -> Result<Vec<DriverLocation>, AppError> {
    let mut driver_locations = Vec::with_capacity(driver_ids.len());

    let driver_last_known_location =
        get_all_driver_last_locations(&data.redis, &driver_ids).await?;

    for (driver_id, driver_last_known_location) in
        driver_ids.iter().zip(driver_last_known_location.iter())
    {
        if let Some(driver_last_known_location) = driver_last_known_location {
            let driver_location = DriverLocation {
                driver_id: driver_id.to_owned(),
                lat: driver_last_known_location.location.lat,
                lon: driver_last_known_location.location.lon,
                coordinates_calculated_at: driver_last_known_location.timestamp,
                created_at: driver_last_known_location.timestamp,
                updated_at: driver_last_known_location.timestamp,
                merchant_id: driver_last_known_location.merchant_id.to_owned(),
            };
            driver_locations.push(driver_location);
        } else {
            warn!(
                "Driver last known location not found for DriverId : {:?}",
                driver_id
            );
        }
    }

    Ok(driver_locations)
}
