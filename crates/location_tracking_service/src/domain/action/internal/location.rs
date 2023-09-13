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
use shared::{tools::error::AppError, utils::logger::*};

#[allow(clippy::too_many_arguments)]
async fn search_nearby_drivers_with_vehicle(
    data: Data<AppState>,
    merchant_id: MerchantId,
    city: CityName,
    vehicle: VehicleType,
    bucket: u64,
    location: Point,
    radius: Radius,
    on_ride: Option<bool>,
) -> Result<Vec<DriverLocation>, AppError> {
    let nearby_drivers = get_drivers_within_radius(
        data.clone(),
        &merchant_id,
        &city,
        &vehicle,
        &bucket,
        location,
        radius,
        on_ride,
    )
    .await?;

    let driver_last_locs = get_all_driver_last_locations(data, &nearby_drivers).await?;

    let resp = nearby_drivers
        .iter()
        .zip(driver_last_locs.iter())
        .map(|(driver, last_location_update_ts)| {
            let driver_loc = DriverLocation {
                driver_id: driver.driver_id.clone(),
                lat: driver.location.lat,
                lon: driver.location.lon,
                coordinates_calculated_at: last_location_update_ts.unwrap_or(Utc::now()),
                created_at: last_location_update_ts.unwrap_or(Utc::now()),
                updated_at: last_location_update_ts.unwrap_or(Utc::now()),
                merchant_id: merchant_id.clone(),
            };
            driver_loc
        })
        .collect::<Vec<DriverLocation>>();

    Ok(resp)
}

pub async fn get_nearby_drivers(
    data: Data<AppState>,
    request_body: NearbyDriversRequest,
) -> Result<NearbyDriverResponse, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    let current_bucket = get_current_bucket(data.bucket_size)?;

    match request_body.clone().vehicle_type {
        None => {
            let mut resp: Vec<DriverLocation> = Vec::new();

            for vehicle in VehicleType::iter() {
                let nearby_drivers = search_nearby_drivers_with_vehicle(
                    data.clone(),
                    request_body.clone().merchant_id,
                    city.clone(),
                    vehicle.clone(),
                    current_bucket,
                    Point {
                        lat: request_body.clone().lat,
                        lon: request_body.clone().lon,
                    },
                    request_body.clone().radius,
                    request_body.on_ride,
                )
                .await;
                match nearby_drivers {
                    Ok(nearby_drivers) => {
                        resp.extend(nearby_drivers.iter().cloned());
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
                request_body.on_ride,
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
    let mut driver_locations = Vec::new();

    for driver_id in driver_ids {
        let driver_details = get_driver_location(data.clone(), &driver_id).await;
        if let Ok(driver_details) = driver_details {
            let driver_location = DriverLocation {
                driver_id: driver_id.clone(),
                lat: driver_details.location.lat,
                lon: driver_details.location.lon,
                coordinates_calculated_at: driver_details.timestamp,
                created_at: driver_details.timestamp,
                updated_at: driver_details.timestamp,
                merchant_id: driver_details.merchant_id,
            };
            driver_locations.push(driver_location);
        }
    }

    Ok(driver_locations)
}
