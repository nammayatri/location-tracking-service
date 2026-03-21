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
    group_id: &Option<String>,
    group_id2: &Option<String>,
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
        .map(|driver| driver.driver_id.clone())
        .collect();

    // Helper to extract last-known-location metadata (all Copy fields)
    let extract_location_meta = |loc: &Option<DriverLastKnownLocation>| -> (TimeStamp, Option<Direction>, Option<VehicleType>) {
        loc.as_ref()
            .map(|l| (l.timestamp, l.bear, l.vehicle_type))
            .unwrap_or((TimeStamp(Utc::now()), None, None))
    };

    let driver_last_known_location = get_all_driver_last_locations(redis, &driver_ids).await?;

    let resp = if [VehicleType::BusAc, VehicleType::BusNonAc]
        .iter()
        .any(|vehicle_type| vehicle_type == vehicle)
    {
        let driver_ride_details =
            get_all_driver_ride_details(redis, &driver_ids, merchant_id).await?;

        let resp = nearby_drivers
            .into_iter()
            .zip(driver_last_known_location.into_iter())
            .zip(driver_ride_details.into_iter())
            .filter_map(
                |((driver, driver_last_known_location), driver_ride_detail)| {
                    if let (
                        Some(RideInfo::Bus {
                            group_id: Some(ref grp_id),
                            ..
                        }),
                        Some(req_group_id),
                    ) = (
                        driver_ride_detail
                            .as_ref()
                            .and_then(|d| d.ride_info.as_ref()),
                        group_id.as_ref(),
                    ) {
                        if grp_id != req_group_id {
                            return None;
                        }
                    }

                    let (last_location_update_ts, bear, vehicle_type) =
                        extract_location_meta(&driver_last_known_location);
                    Some(DriverLocationDetail {
                        driver_id: driver.driver_id,
                        lat: driver.location.lat,
                        lon: driver.location.lon,
                        coordinates_calculated_at: last_location_update_ts,
                        created_at: last_location_update_ts,
                        updated_at: last_location_update_ts,
                        merchant_id: merchant_id.clone(),
                        group_id: driver_last_known_location
                            .as_ref()
                            .and_then(|loc| loc.group_id.clone()),
                        group_id2: driver_last_known_location
                            .as_ref()
                            .and_then(|loc| loc.group_id2.clone()),
                        ride_details: driver_ride_detail.map(|rd| RideDetailsApiEntity {
                            ride_id: rd.ride_id,
                            ride_status: rd.ride_status,
                            ride_info: rd.ride_info,
                        }),
                        bear,
                        vehicle_type,
                    })
                },
            )
            .collect::<Vec<DriverLocationDetail>>();
        resp
    } else if [VehicleType::VipEscort, VehicleType::VipOfficer]
        .iter()
        .any(|vehicle_type| vehicle_type == vehicle)
    {
        let driver_ride_details =
            get_all_driver_ride_details(redis, &driver_ids, merchant_id).await?;

        let resp = nearby_drivers
            .into_iter()
            .zip(driver_last_known_location.into_iter())
            .zip(driver_ride_details.into_iter())
            .filter_map(
                |((driver, driver_last_known_location), driver_ride_detail)| {
                    if let (
                        Some(RideInfo::Pilot {
                            group_id: Some(ref grp_id),
                            ..
                        }),
                        Some(req_group_id),
                    ) = (
                        driver_ride_detail
                            .as_ref()
                            .and_then(|d| d.ride_info.as_ref()),
                        group_id.as_ref(),
                    ) {
                        if grp_id != req_group_id {
                            return None;
                        }
                    }

                    let (last_location_update_ts, bear, vehicle_type) =
                        extract_location_meta(&driver_last_known_location);
                    Some(DriverLocationDetail {
                        driver_id: driver.driver_id,
                        lat: driver.location.lat,
                        lon: driver.location.lon,
                        coordinates_calculated_at: last_location_update_ts,
                        created_at: last_location_update_ts,
                        updated_at: last_location_update_ts,
                        merchant_id: merchant_id.clone(),
                        group_id: driver_last_known_location
                            .as_ref()
                            .and_then(|loc| loc.group_id.clone()),
                        group_id2: driver_last_known_location
                            .as_ref()
                            .and_then(|loc| loc.group_id2.clone()),
                        ride_details: driver_ride_detail.map(|rd| RideDetailsApiEntity {
                            ride_id: rd.ride_id,
                            ride_status: rd.ride_status,
                            ride_info: rd.ride_info,
                        }),
                        bear,
                        vehicle_type,
                    })
                },
            )
            .collect::<Vec<DriverLocationDetail>>();
        resp
    } else {
        let make_detail =
            |driver: DriverLocationPoint,
             driver_last_known_location: Option<DriverLastKnownLocation>| {
                let (last_location_update_ts, bear, vehicle_type) =
                    extract_location_meta(&driver_last_known_location);
                DriverLocationDetail {
                    driver_id: driver.driver_id,
                    lat: driver.location.lat,
                    lon: driver.location.lon,
                    coordinates_calculated_at: last_location_update_ts,
                    created_at: last_location_update_ts,
                    updated_at: last_location_update_ts,
                    merchant_id: merchant_id.clone(),
                    group_id: driver_last_known_location
                        .as_ref()
                        .and_then(|loc| loc.group_id.clone()),
                    group_id2: driver_last_known_location
                        .as_ref()
                        .and_then(|loc| loc.group_id2.clone()),
                    ride_details: None,
                    bear,
                    vehicle_type,
                }
            };

        let resp = nearby_drivers
            .into_iter()
            .zip(driver_last_known_location.into_iter())
            .filter_map(|(driver, driver_last_known_location)| {
                let mb_last_known_group_id = driver_last_known_location
                    .as_ref()
                    .and_then(|loc| loc.group_id.as_deref());

                let mb_last_known_group_id2 = driver_last_known_location
                    .as_ref()
                    .and_then(|loc| loc.group_id2.as_deref());

                match (
                    mb_last_known_group_id,
                    mb_last_known_group_id2,
                    group_id.as_deref(),
                    group_id2.as_deref(),
                ) {
                    // no filters by group_id (default)
                    (_, _, None, None) => Some(make_detail(driver, driver_last_known_location)),

                    // filter by group_id
                    (Some(last_known_group_id), _, Some(req_group_id), None)
                        if last_known_group_id == req_group_id =>
                    {
                        Some(make_detail(driver, driver_last_known_location))
                    }

                    // filter by group_id2
                    (_, Some(last_known_group_id2), None, Some(req_group_id2))
                        if last_known_group_id2 == req_group_id2 =>
                    {
                        Some(make_detail(driver, driver_last_known_location))
                    }

                    // filter by both group_id and group_id2
                    (
                        Some(last_known_group_id),
                        Some(last_known_group_id2),
                        Some(req_group_id),
                        Some(req_group_id2),
                    ) if last_known_group_id == req_group_id
                        && last_known_group_id2 == req_group_id2 =>
                    {
                        Some(make_detail(driver, driver_last_known_location))
                    }

                    _ => None,
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
        group_id,
        group_id2,
    }: NearbyDriversRequest,
) -> Result<NearbyDriverResponse, AppError> {
    let city = get_city(&lat, &lon, &data.polygon)?;

    let current_bucket = get_bucket_from_timestamp(&data.bucket_size, TimeStamp(Utc::now()));

    let vehicles: Vec<VehicleType> = match vehicle_type {
        None => VehicleType::iter().collect(),
        Some(vehicles) => vehicles,
    };

    // Query all vehicle types concurrently instead of sequentially
    let futures: Vec<_> = vehicles
        .iter()
        .map(|vehicle| {
            search_nearby_drivers_with_vehicle(
                &data.redis,
                &data.nearby_bucket_threshold,
                &merchant_id,
                &city,
                vehicle,
                &current_bucket,
                Point { lat, lon },
                &radius,
                &group_id,
                &group_id2,
            )
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut resp: Vec<DriverLocationDetail> = Vec::new();
    for (vehicle, result) in vehicles.iter().zip(results) {
        match result {
            Ok(nearby_drivers) => resp.extend(nearby_drivers),
            Err(err) => {
                error!(tag="[Nearby Drivers]", vehicle = %vehicle, "{:?}", err)
            }
        }
    }

    Ok(resp)
}

#[macros::measure_duration]
pub async fn get_drivers_location(
    data: Data<AppState>,
    driver_ids: Vec<DriverId>,
) -> Result<Vec<DriverLocationDetail>, AppError> {
    let mut driver_locations = Vec::with_capacity(driver_ids.len());

    let driver_last_known_location =
        get_all_driver_last_locations(&data.redis, &driver_ids).await?;

    for (driver_id, driver_last_known_location) in driver_ids
        .into_iter()
        .zip(driver_last_known_location.into_iter())
    {
        if let Some(loc) = driver_last_known_location {
            driver_locations.push(DriverLocationDetail {
                driver_id,
                lat: loc.location.lat,
                lon: loc.location.lon,
                coordinates_calculated_at: loc.timestamp,
                created_at: loc.timestamp,
                updated_at: loc.timestamp,
                merchant_id: loc.merchant_id,
                group_id: loc.group_id,
                group_id2: loc.group_id2,
                ride_details: None,
                bear: loc.bear,
                vehicle_type: loc.vehicle_type,
            });
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
            &details.detection_state,
            &details.anti_detection_state,
            &details.violation_trigger_flag,
            &details.driver_pickup_distance,
            &details.driver_last_known_location.bear,
            &details.driver_last_known_location.vehicle_type,
            &details.driver_last_known_location.group_id,
            &details.driver_last_known_location.group_id2,
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

pub async fn driver_queue_position(
    data: Data<AppState>,
    special_location_id: String,
    vehicle_type: String,
    driver_id: String,
) -> Result<DriverQueuePositionResponse, AppError> {
    let queue_size = get_queue_size(&data.redis, &special_location_id, &vehicle_type).await?;
    let rank =
        get_driver_queue_position(&data.redis, &special_location_id, &vehicle_type, &driver_id)
            .await?;
    let offset = data.queue_position_range_offset;
    let queue_position_range = rank.map(|r| {
        let pos = r + 1; // 1-indexed
        let low = if pos <= offset { 1 } else { pos - offset };
        let high = std::cmp::min(pos + offset, queue_size);
        (low, high)
    });
    Ok(DriverQueuePositionResponse {
        queue_position_range,
        queue_size,
    })
}

pub async fn manual_queue_remove(
    data: Data<AppState>,
    special_location_id: String,
    vehicle_type: String,
    merchant_id: String,
    driver_id: String,
) -> Result<APISuccess, AppError> {
    remove_driver_from_queue(&data.redis, &special_location_id, &vehicle_type, &driver_id).await?;
    delete_driver_queue_tracking(&data.redis, &merchant_id, &driver_id).await?;
    Ok(APISuccess::default())
}

pub async fn manual_queue_add(
    data: Data<AppState>,
    special_location_id: String,
    vehicle_type: String,
    merchant_id: String,
    driver_id: String,
    queue_position: u64, // 1-indexed
) -> Result<APISuccess, AppError> {
    // First remove the driver if already in a different queue
    let existing_tracking =
        get_driver_queue_tracking(&data.redis, &merchant_id, &driver_id).await?;
    if let Some(old) = &existing_tracking {
        if old.special_location_id != special_location_id || old.vehicle_type != vehicle_type {
            remove_driver_from_queue(
                &data.redis,
                &old.special_location_id,
                &old.vehicle_type,
                &driver_id,
            )
            .await?;
        }
    }

    // Also remove from the target queue if already present (to re-insert at new position)
    remove_driver_from_queue(&data.redis, &special_location_id, &vehicle_type, &driver_id).await?;

    // Recompute queue size after removal
    let queue_size_after_removal =
        get_queue_size(&data.redis, &special_location_id, &vehicle_type).await?;

    // Compute the score for the desired position
    let target_rank = if queue_position == 0 {
        0u64
    } else {
        (queue_position - 1).min(queue_size_after_removal)
    };

    let score = if queue_size_after_removal == 0 {
        // Empty queue — use current timestamp
        Utc::now().timestamp() as f64
    } else if target_rank == 0 {
        // Insert at the front — use score slightly before the first member
        let scores =
            get_queue_scores_at_range(&data.redis, &special_location_id, &vehicle_type, 0, 0)
                .await?;
        if let Some((_, first_score)) = scores.first() {
            first_score - 1.0
        } else {
            Utc::now().timestamp() as f64
        }
    } else if target_rank >= queue_size_after_removal {
        // Insert at the end — use score slightly after the last member
        let last = queue_size_after_removal as i64 - 1;
        let scores =
            get_queue_scores_at_range(&data.redis, &special_location_id, &vehicle_type, last, last)
                .await?;
        if let Some((_, last_score)) = scores.first() {
            last_score + 1.0
        } else {
            Utc::now().timestamp() as f64
        }
    } else {
        // Insert between ranks (target_rank - 1) and target_rank
        let before = target_rank as i64 - 1;
        let at = target_rank as i64;
        let scores =
            get_queue_scores_at_range(&data.redis, &special_location_id, &vehicle_type, before, at)
                .await?;
        if scores.len() == 2 {
            (scores[0].1 + scores[1].1) / 2.0
        } else {
            Utc::now().timestamp() as f64
        }
    };

    add_driver_to_queue_force(
        &data.redis,
        &special_location_id,
        &vehicle_type,
        &driver_id,
        score,
        data.queue_expiry_seconds as i64,
    )
    .await?;

    // Set tracking key
    set_driver_queue_tracking(
        &data.redis,
        &merchant_id,
        &driver_id,
        &DriverQueueTracking {
            special_location_id,
            vehicle_type,
        },
    )
    .await?;

    Ok(APISuccess::default())
}

pub async fn get_special_location_drivers(
    data: Data<AppState>,
    special_location_id: String,
) -> Result<SpecialLocationDriversResponse, AppError> {
    if !data.enable_special_location_bucketing {
        return Ok(SpecialLocationDriversResponse { driver_ids: vec![] });
    }
    let current_bucket = get_bucket_from_timestamp(&data.bucket_size, TimeStamp(Utc::now()));
    let driver_ids = get_drivers_in_special_location(
        &data.redis,
        &special_location_id,
        &current_bucket,
        &data.nearby_bucket_threshold,
    )
    .await?;
    Ok(SpecialLocationDriversResponse { driver_ids })
}
