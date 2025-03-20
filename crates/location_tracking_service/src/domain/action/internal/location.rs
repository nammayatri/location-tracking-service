/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::all)]
use std::collections::HashMap;

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
) -> Result<Vec<DriverLocationDetail>, AppError> {
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

    let resp = if [VehicleType::BusAc, VehicleType::BusNonAc]
        .iter()
        .any(|vehicle_type| vehicle_type == vehicle)
    {
        let driver_ride_details =
            get_all_driver_ride_details(redis, &driver_ids, merchant_id).await?;

        let resp = nearby_drivers
            .iter()
            .zip(driver_last_known_location.iter())
            .zip(driver_ride_details.iter())
            .map(
                |((driver, driver_last_known_location), driver_ride_detail)| {
                    let (last_location_update_ts, bear, vehicle_type) = driver_last_known_location
                        .as_ref()
                        .map(|driver_last_known_location| {
                            (
                                driver_last_known_location.timestamp,
                                driver_last_known_location.bear,
                                driver_last_known_location.vehicle_type,
                            )
                        })
                        .unwrap_or((TimeStamp(Utc::now()), None, None));
                    DriverLocationDetail {
                        driver_id: driver.driver_id.to_owned(),
                        lat: driver.location.lat,
                        lon: driver.location.lon,
                        coordinates_calculated_at: last_location_update_ts,
                        created_at: last_location_update_ts,
                        updated_at: last_location_update_ts,
                        merchant_id: merchant_id.to_owned(),
                        ride_details: driver_ride_detail.clone(),
                        bear,
                        vehicle_type,
                    }
                },
            )
            .collect::<Vec<DriverLocationDetail>>();
        resp
    } else {
        let resp = nearby_drivers
            .iter()
            .zip(driver_last_known_location.iter())
            .map(|(driver, driver_last_known_location)| {
                let (last_location_update_ts, bear, vehicle_type) = driver_last_known_location
                    .as_ref()
                    .map(|driver_last_known_location| {
                        (
                            driver_last_known_location.timestamp,
                            driver_last_known_location.bear,
                            driver_last_known_location.vehicle_type,
                        )
                    })
                    .unwrap_or((TimeStamp(Utc::now()), None, None));
                DriverLocationDetail {
                    driver_id: driver.driver_id.to_owned(),
                    lat: driver.location.lat,
                    lon: driver.location.lon,
                    coordinates_calculated_at: last_location_update_ts,
                    created_at: last_location_update_ts,
                    updated_at: last_location_update_ts,
                    merchant_id: merchant_id.to_owned(),
                    ride_details: None,
                    bear,
                    vehicle_type,
                }
            })
            .collect::<Vec<DriverLocationDetail>>();
        resp
    };

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
            let mut resp: Vec<DriverLocationDetail> = Vec::new();

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
            let mut resp: Vec<DriverLocationDetail> = Vec::new();
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
) -> Result<Vec<DriverLocationDetail>, AppError> {
    let mut driver_locations = Vec::with_capacity(driver_ids.len());

    let driver_last_known_location =
        get_all_driver_last_locations(&data.redis, &driver_ids).await?;

    for (driver_id, driver_last_known_location) in
        driver_ids.iter().zip(driver_last_known_location.iter())
    {
        if let Some(driver_last_known_location) = driver_last_known_location {
            let driver_location = DriverLocationDetail {
                driver_id: driver_id.to_owned(),
                lat: driver_last_known_location.location.lat,
                lon: driver_last_known_location.location.lon,
                coordinates_calculated_at: driver_last_known_location.timestamp,
                created_at: driver_last_known_location.timestamp,
                updated_at: driver_last_known_location.timestamp,
                merchant_id: driver_last_known_location.merchant_id.to_owned(),
                ride_details: None,
                bear: driver_last_known_location.bear,
                vehicle_type: driver_last_known_location.vehicle_type.clone(),
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

pub async fn driver_block_till(
    data: Data<AppState>,
    request_body: DriverBlockTillRequest,
) -> Result<APISuccess, AppError> {
    let driver_details = get_driver_location(&data.redis, &request_body.driver_id).await?;

    if let Some(details) = driver_details {
        set_driver_last_location_update(
            &data.redis,
            &data.last_location_timstamp_expiry,
            &request_body.driver_id,
            &request_body.merchant_id,
            &details.driver_last_known_location.location,
            &details.driver_last_known_location.timestamp,
            &Some(request_body.block_till),
            details.stop_detection,
            &None::<RideStatus>,
            &None,
            &None,
            &details.driver_last_known_location.bear,
            // travelled_distance.to_owned(),
            &details.driver_last_known_location.vehicle_type,
        )
        .await?;
    };
    Ok(APISuccess::default())
}

#[macros::measure_duration]
pub async fn track_vehicles(
    data: Data<AppState>,
    request_body: TrackVehicleRequest,
) -> Result<Vec<TrackVehicleResponse>, AppError> {
    let track_vehicle_info = match request_body {
        TrackVehicleRequest::RouteCode(route_code) => {
            get_route_location(&data.redis, &route_code).await?
        }
        TrackVehicleRequest::TripCodes(trip_codes) => {
            let mut track_vehicles_info = HashMap::new();
            for trip_code in trip_codes {
                let location = get_trip_location(&data.redis, &trip_code).await?;
                for (vehicle_number, vehicle_info) in location.into_iter() {
                    track_vehicles_info.insert(vehicle_number, vehicle_info);
                }
            }
            track_vehicles_info
        }
    };

    Ok(track_vehicle_info
        .into_iter()
        .map(|(vehicle_number, vehicle_info)| TrackVehicleResponse {
            vehicle_number,
            vehicle_info,
        })
        .collect())
}
