/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::all)]
use crate::common::detection::*;
use crate::common::stop_detection::*;
use crate::common::utils::{
    distance_between_in_meters, estimated_upcoming_stops_eta, get_base_vehicle_type, get_city,
    get_upcoming_stops_by_route_code, is_blacklist_for_special_zone,
};
use crate::common::{sliding_window_rate_limiter::sliding_window_limiter, types::*};
use crate::domain::types::ui::location::{DriverLocationResponse, UpdateDriverLocationRequest};
use crate::environment::AppState;
use crate::kafka::producers::kafka_stream_updates;
use crate::outbound::external::driver_source_departed;
use crate::outbound::external::trigger_detection_alert;
use crate::outbound::external::{
    authenticate_dobpp, bulk_location_update_dobpp, driver_reached_destination, trigger_fcm_bap,
    trigger_fcm_dobpp, trigger_stop_detection_event,
};
use crate::outbound::types::ViolationDetectionReq;
use crate::redis::{commands::*, keys::*};
use crate::tools::error::AppError;
use crate::tools::prometheus::MEASURE_DURATION;
use actix::Arbiter;
use actix_web::{web::Data, HttpResponse};
use chrono::Utc;
use futures::future::join_all;
use futures::Future;
use reqwest::Url;
use shared::measure_latency_duration;
use shared::redis::types::RedisConnectionPool;
use std::collections::HashMap;
use std::env::var;
use std::pin::Pin;
use strum::IntoEnumIterator;
use tracing::{debug, info, warn};

#[macros::measure_duration]
async fn get_driver_id_from_authentication(
    redis: &RedisConnectionPool,
    auth_url: &Url,
    auth_api_key: &str,
    auth_token_expiry: &u32,
    token: &Token,
) -> Result<(DriverId, MerchantId, MerchantOperatingCityId), AppError> {
    match get_driver_id(redis, token).await? {
        Some(auth_data) => Ok((
            auth_data.driver_id,
            auth_data.merchant_id,
            auth_data.merchant_operating_city_id,
        )),
        None => {
            let response = authenticate_dobpp(auth_url, token.0.as_str(), auth_api_key).await?;
            set_driver_id(
                redis,
                auth_token_expiry,
                token,
                response.driver_id.to_owned(),
                response.merchant_id.to_owned(),
                response.merchant_operating_city_id.to_owned(),
            )
            .await?;
            Ok((
                response.driver_id,
                response.merchant_id,
                response.merchant_operating_city_id,
            ))
        }
    }
}

#[macros::measure_duration]
fn get_filtered_driver_locations(
    last_known_location: Option<&DriverLastKnownLocation>,
    mut locations: Vec<UpdateDriverLocationRequest>,
    min_location_accuracy: Accuracy,
    driver_location_accuracy_buffer: f64,
) -> (Vec<(UpdateDriverLocationRequest, LocationType)>, bool) {
    locations.dedup_by(|a, b| a.pt.lat == b.pt.lat && a.pt.lon == b.pt.lon);

    locations.into_iter().fold(
        (Vec::new(), false),
        |(mut acc_locations, mut any_location_unfiltered), location| {
            let location_type = if location.acc.or(Some(Accuracy(0.0)))
                <= Some(min_location_accuracy)
                && last_known_location
                    .map(|last_known_location| {
                        distance_between_in_meters(&last_known_location.location, &location.pt)
                            > driver_location_accuracy_buffer
                    })
                    .unwrap_or(true)
            {
                LocationType::UNFILTERED
            } else {
                LocationType::FILTERED
            };

            any_location_unfiltered =
                any_location_unfiltered || location_type == LocationType::UNFILTERED;

            acc_locations.push((location, location_type));

            (acc_locations, any_location_unfiltered)
        },
    )
}

#[macros::measure_duration]
pub async fn update_driver_location(
    token: Token,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    mut locations: Vec<UpdateDriverLocationRequest>,
    driver_mode: DriverMode,
) -> Result<HttpResponse, AppError> {
    let current_ts = Utc::now();
    let (driver_id, merchant_id, merchant_operating_city_id) = if var("DEV").is_ok() {
        (
            DriverId(token.to_owned().inner()),
            MerchantId("dev".to_string()),
            MerchantOperatingCityId("dev".to_string()),
        )
    } else {
        get_driver_id_from_authentication(
            &data.redis,
            &data.auth_url,
            &data.auth_api_key,
            &data.auth_token_expiry,
            &token,
        )
        .await?
    };

    if locations.len() > data.batch_size as usize {
        warn!(
            "Way points more than {} points => {} points",
            data.batch_size,
            locations.len()
        );
    }

    locations.sort_by(|a, b| {
        let TimeStamp(a_ts) = a.ts;
        let TimeStamp(b_ts) = b.ts;
        (a_ts).cmp(&b_ts)
    });

    let driver_location_details = get_driver_location(&data.redis, &driver_id).await?;

    if let Some(driver_location_details) = driver_location_details.as_ref() {
        let curr_time = TimeStamp(Utc::now());

        if driver_location_details.blocked_till > Some(curr_time) {
            return Err(AppError::DriverBlocked);
        }
    }

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .into_iter()
        .filter(|location| {
            driver_location_details
                .as_ref()
                .map(|driver_location_details| {
                    location.ts >= driver_location_details.driver_last_known_location.timestamp
                })
                .unwrap_or(true)
        })
        .collect();

    let latest_driver_location = if let Some(location) = locations.last() {
        location.to_owned()
    } else {
        return Ok(HttpResponse::Ok().finish());
    };

    info!(
        tag = "[Location Updates]",
        "Got location updates for Driver Id : {:?} : {:?}", &driver_id, &locations
    );

    let city = get_city(
        &latest_driver_location.pt.lat,
        &latest_driver_location.pt.lon,
        &data.polygon,
    )?;

    sliding_window_limiter(
        &data.redis,
        &sliding_rate_limiter_key(&driver_id, &city, &merchant_id),
        data.location_update_limit,
        data.location_update_interval as u32,
    )
    .await?;

    with_lock_redis(
        &data.redis,
        driver_processing_location_update_lock_key(&driver_id, &merchant_id, &city),
        60,
        process_driver_locations,
        (
            data.clone(),
            locations,
            latest_driver_location,
            TimeStamp(current_ts),
            driver_location_details,
            driver_id,
            merchant_id,
            merchant_operating_city_id,
            vehicle_type,
            city,
            driver_mode,
        ),
    )
    .await?;
    Ok(HttpResponse::Ok().finish())
}

#[macros::measure_duration]
#[allow(clippy::type_complexity)]
async fn process_driver_locations(
    args: (
        Data<AppState>,
        Vec<UpdateDriverLocationRequest>,
        UpdateDriverLocationRequest,
        TimeStamp,
        Option<DriverAllDetails>,
        DriverId,
        MerchantId,
        MerchantOperatingCityId,
        VehicleType,
        CityName,
        DriverMode,
    ),
) -> Result<(), AppError> {
    let (
        data,
        locations,
        latest_driver_location,
        current_ts,
        driver_location_details,
        driver_id,
        merchant_id,
        merchant_operating_city_id,
        vehicle_type,
        city,
        driver_mode,
    ) = args;

    let driver_ride_details = get_ride_details(&data.redis, &driver_id, &merchant_id).await?;

    let driver_ride_id = driver_ride_details
        .as_ref()
        .map(|ride_details| ride_details.ride_id.to_owned());

    let driver_ride_status = driver_ride_details
        .as_ref()
        .map(|ride_details| ride_details.ride_status.to_owned());

    let driver_ride_info = driver_ride_details
        .as_ref()
        .map(|ride_details| ride_details.ride_info.to_owned())
        .flatten();

    let driver_ride_notification_status = driver_location_details
        .as_ref()
        .map(|ride_details| ride_details.ride_notification_status)
        .flatten();

    let driver_pickup_distance = driver_location_details
        .as_ref()
        .map(|ride_details| ride_details.driver_pickup_distance)
        .flatten();

    let driver_last_known_location =
        driver_location_details
            .as_ref()
            .map(|driver_location_details| {
                driver_location_details
                    .driver_last_known_location
                    .to_owned()
            });

    let TimeStamp(latest_driver_location_ts) = latest_driver_location.ts;
    let latest_driver_location_ts = if latest_driver_location_ts > current_ts.inner() {
        warn!(
            "Latest driver location timestamp in future => {}, Switching to current time => {}",
            latest_driver_location_ts,
            current_ts.inner()
        );
        current_ts
    } else {
        TimeStamp(latest_driver_location_ts)
    };
    let base_vehicle_type = get_base_vehicle_type(&vehicle_type);

    let (
        detection_state,
        anti_detection_state,
        violation_trigger_flag,
        violation_detection_requests,
    ): (
        ViolationDetectionStateMap,
        ViolationDetectionStateMap,
        ViolationDetectionTriggerMap,
        Vec<ViolationDetectionReq>,
    ) = {
        if let (Some(ride_status), Some(ride_id)) = (
            driver_location_details
                .as_ref()
                .and_then(|d| d.ride_status.clone()),
            driver_ride_id.as_ref(),
        ) {
            DetectionType::iter().fold(
                (
                    driver_location_details
                        .as_ref()
                        .map(|driver_location_details| {
                            driver_location_details.detection_state.to_owned()
                        })
                        .flatten()
                        .unwrap_or_default(),
                    driver_location_details
                        .as_ref()
                        .map(|driver_location_details| {
                            driver_location_details.anti_detection_state.to_owned()
                        })
                        .flatten()
                        .unwrap_or_default(),
                    driver_location_details
                        .as_ref()
                        .map(|driver_location_details| {
                            driver_location_details.violation_trigger_flag.to_owned()
                        })
                        .flatten()
                        .unwrap_or_default(),
                    Vec::new(),
                ),
                |(
                    mut detection_violation_state_map,
                    mut detection_anti_violation_state_map,
                    mut violation_trigger_flag_map,
                    mut violation_detection_requests,
                ),
                 detection_type| {
                    let context = DetectionContext {
                        ride_id: ride_id.clone(),
                        driver_id: driver_id.clone(),
                        location: latest_driver_location.pt.clone(),
                        timestamp: latest_driver_location_ts.clone(),
                        speed: latest_driver_location
                            .v
                            .map(|v| SpeedInMeterPerSecond(v.inner())),
                        ride_status: ride_status.to_owned(),
                        ride_info: driver_ride_info.clone(),
                        vehicle_type: vehicle_type.to_owned(),
                        accuracy: latest_driver_location.acc.unwrap_or(Accuracy(0.0)),
                    };

                    if let (
                        Some(detection_violation_config),
                        Some(detection_anti_violation_config),
                    ) = (
                        data.detection_violation_config
                            .get(&base_vehicle_type)
                            .and_then(|inner_map| inner_map.get(&detection_type)),
                        data.detection_anti_violation_config
                            .get(&base_vehicle_type)
                            .and_then(|inner_map| inner_map.get(&detection_type)),
                    ) {
                        // Skip detection if accuracy is poor
                        if context.accuracy > data.min_location_accuracy {
                            return (
                                detection_violation_state_map,
                                detection_anti_violation_state_map,
                                violation_trigger_flag_map,
                                violation_detection_requests,
                            );
                        }

                        if let (
                            Some(detection_violation_state),
                            Some(detection_anti_violation_state),
                            violation_trigger_flag,
                            violation_detection_req,
                        ) = check(
                            detection_violation_config,
                            detection_anti_violation_config,
                            context,
                            detection_violation_state_map.get(&detection_type).cloned(),
                            detection_anti_violation_state_map
                                .get(&detection_type)
                                .cloned(),
                            violation_trigger_flag_map
                                .get(&detection_type)
                                .cloned()
                                .flatten(),
                            data.clone(),
                        ) {
                            detection_violation_state_map
                                .insert(detection_type.clone(), detection_violation_state);
                            detection_anti_violation_state_map
                                .insert(detection_type.clone(), detection_anti_violation_state);
                            violation_trigger_flag_map
                                .insert(detection_type.clone(), violation_trigger_flag);

                            if let Some(violation_detection_request) = violation_detection_req {
                                violation_detection_requests.push(violation_detection_request)
                            }
                        }
                    }
                    (
                        detection_violation_state_map,
                        detection_anti_violation_state_map,
                        violation_trigger_flag_map,
                        violation_detection_requests,
                    )
                },
            )
        } else {
            (
                HashMap::default(),
                HashMap::default(),
                HashMap::default(),
                Vec::new(),
            )
        }
    };

    let (stop_detected, stop_detection) =
        if let Some(stop_detection_config) = data.stop_detection.get(&base_vehicle_type) {
            let stop_detection_config = match driver_ride_status {
                Some(RideStatus::NEW) => stop_detection_config.get(&RideStatus::NEW),
                Some(RideStatus::INPROGRESS) => stop_detection_config.get(&RideStatus::INPROGRESS),
                _ => None,
            };
            if let Some(config) = stop_detection_config {
                if driver_ride_status == Some(RideStatus::NEW)
                    || (config.enable_onride_stop_detection
                        && driver_ride_status == Some(RideStatus::INPROGRESS))
                {
                    detect_stop(
                        driver_location_details
                            .as_ref()
                            .map(|driver_location_details| {
                                driver_location_details.stop_detection.to_owned()
                            })
                            .flatten(),
                        DriverLocation {
                            location: latest_driver_location.pt.to_owned(),
                            timestamp: latest_driver_location.ts,
                        },
                        latest_driver_location.v,
                        config,
                    )
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

    let (
        driver_ride_notification_status,
        is_driver_ride_notification_status_changed,
        driver_pickup_distance,
    ) = (|| {
        if let Some(RideInfo::Car {
            pickup_location,
            min_distance_between_two_points: _,
        }) = driver_ride_info.as_ref()
        {
            let pickup_distance =
                distance_between_in_meters(pickup_location, &latest_driver_location.pt);
            if let Some(ride_notification_status) = driver_ride_notification_status {
                if let Some(driver_pickup_distance) = driver_pickup_distance {
                    if (driver_pickup_distance.inner() as f64 - 50.0) > pickup_distance {
                        if pickup_distance <= data.pickup_notification_threshold
                            && RideNotificationStatus::DriverReached > ride_notification_status
                        {
                            return (
                                Some(RideNotificationStatus::DriverReached),
                                true,
                                Some(driver_pickup_distance),
                            );
                        } else if pickup_distance <= data.arriving_notification_threshold
                            && RideNotificationStatus::DriverReaching > ride_notification_status
                        {
                            return (
                                Some(RideNotificationStatus::DriverReaching),
                                true,
                                Some(driver_pickup_distance),
                            );
                        } else if RideNotificationStatus::DriverOnTheWay > ride_notification_status
                        {
                            return (
                                Some(RideNotificationStatus::DriverOnTheWay),
                                true,
                                Some(driver_pickup_distance),
                            );
                        }
                    }
                    (
                        driver_ride_notification_status,
                        false,
                        Some(driver_pickup_distance),
                    )
                } else {
                    (
                        Some(ride_notification_status),
                        false,
                        Some(Meters(pickup_distance as u32)),
                    )
                }
            } else {
                (
                    Some(RideNotificationStatus::Idle),
                    false,
                    Some(Meters(pickup_distance as u32)),
                )
            }
        } else {
            (None, false, None)
        }
    })();

    let (locations, upcoming_stops) = match driver_ride_info.as_ref() {
        Some(RideInfo::Bus {
            route_code,
            bus_number,
            ..
        }) => {
            let mut all_tasks: Vec<Pin<Box<dyn Future<Output = Result<(), AppError>>>>> =
                Vec::new();

            let upcoming_stops = data.routes.get(route_code).and_then(|route| {
                get_upcoming_stops_by_route_code(route, &latest_driver_location.pt).ok()
            });

            let vehicle_route_location =
                get_route_location_by_vehicle_number(&data.redis, &route_code, &bus_number).await?;

            let upcoming_stops_with_eta = if let Some(upcoming_stops) = upcoming_stops.as_ref() {
                estimated_upcoming_stops_eta(
                    vehicle_route_location
                        .as_ref()
                        .map(|vehicle_route_location| {
                            vehicle_route_location.upcoming_stops.to_owned()
                        })
                        .flatten(),
                    upcoming_stops,
                )
            } else {
                None
            };

            let set_route_location = async {
                set_route_location(
                    &data.redis,
                    &route_code,
                    &bus_number,
                    &latest_driver_location.pt,
                    &latest_driver_location.v,
                    &latest_driver_location_ts,
                    driver_ride_status.to_owned(),
                    upcoming_stops_with_eta,
                )
                .await?;
                Ok(())
            };
            all_tasks.push(Box::pin(set_route_location));

            let set_driver_last_location_update = async {
                set_driver_last_location_update(
                    &data.redis,
                    &data.last_location_timstamp_expiry,
                    &driver_id,
                    &merchant_id,
                    &latest_driver_location.pt,
                    &latest_driver_location_ts,
                    &None,
                    stop_detection,
                    &driver_ride_status,
                    &driver_ride_notification_status,
                    &Some(detection_state),
                    &Some(anti_detection_state),
                    &Some(violation_trigger_flag),
                    &driver_pickup_distance,
                    &None,
                    &Some(vehicle_type.clone()),
                )
                .await?;
                Ok(())
            };
            all_tasks.push(Box::pin(set_driver_last_location_update));

            let send_driver_location_to_drainer = async {
                let _ = &data
                    .sender
                    .send((
                        Dimensions {
                            merchant_id: merchant_id.to_owned(),
                            city: city.to_owned(),
                            vehicle_type: vehicle_type.to_owned(),
                            created_at: Utc::now(),
                        },
                        latest_driver_location.pt.lat,
                        latest_driver_location.pt.lon,
                        latest_driver_location_ts.to_owned(),
                        driver_id.to_owned(),
                    ))
                    .await
                    .map_err(|err| AppError::DrainerPushFailed(err.to_string()))?;
                Ok(())
            };
            all_tasks.push(Box::pin(send_driver_location_to_drainer));

            join_all(all_tasks)
                .await
                .into_iter()
                .try_for_each(Result::from)?;

            (
                locations
                    .into_iter()
                    .map(|loc| (loc, LocationType::UNFILTERED))
                    .collect(),
                upcoming_stops,
            )
        }
        _ => {
            let mut all_tasks: Vec<Pin<Box<dyn Future<Output = Result<(), AppError>>>>> =
                Vec::new();

            let is_blacklist_for_special_zone = is_blacklist_for_special_zone(
                &merchant_id,
                &data.blacklist_merchants,
                &latest_driver_location.pt.lat,
                &latest_driver_location.pt.lon,
                &data.blacklist_polygon,
            );

            if !is_blacklist_for_special_zone {
                let send_driver_location_to_drainer = async {
                    let _ = &data
                        .sender
                        .send((
                            Dimensions {
                                merchant_id: merchant_id.to_owned(),
                                city: city.to_owned(),
                                vehicle_type: vehicle_type.to_owned(),
                                created_at: Utc::now(),
                            },
                            latest_driver_location.pt.lat,
                            latest_driver_location.pt.lon,
                            latest_driver_location_ts.to_owned(),
                            driver_id.to_owned(),
                        ))
                        .await
                        .map_err(|err| AppError::DrainerPushFailed(err.to_string()))?;
                    Ok(())
                };
                all_tasks.push(Box::pin(send_driver_location_to_drainer));
            } else {
                warn!(
                    "Skipping GEOADD for special zone ({:?}) Driver Id : {:?}, Merchant Id : {:?}",
                    latest_driver_location.pt, driver_id, merchant_id
                );
            }
            let driver_location_accuracy_buffer_to_use: f64 = match driver_ride_info {
                Some(RideInfo::Car {
                    min_distance_between_two_points,
                    ..
                }) => min_distance_between_two_points
                    .map(|x| x as f64)
                    .unwrap_or(data.driver_location_accuracy_buffer),
                _ => data.driver_location_accuracy_buffer,
            };

            let (locations, any_location_unfiltered) =
                if let Some(RideStatus::INPROGRESS) = driver_ride_status.as_ref() {
                    let (locations, any_location_unfiltered) = get_filtered_driver_locations(
                        driver_last_known_location.as_ref(),
                        locations,
                        data.min_location_accuracy,
                        driver_location_accuracy_buffer_to_use,
                    );
                    if !locations.is_empty() {
                        if locations.len() > data.batch_size as usize {
                            warn!(
                            "On Ride Way points more than {} points after filtering => {} points",
                            data.batch_size,
                            locations.len()
                        );
                        }
                    } else {
                        warn!(
                        "All On Ride Way Points got filtered, batch size: {}, location_len: {} ",
                        data.batch_size,
                        locations.len()
                    );
                    }
                    (locations, any_location_unfiltered)
                } else {
                    (
                        locations
                            .into_iter()
                            .map(|loc| (loc, LocationType::UNFILTERED))
                            .collect(),
                        true,
                    )
                };

            let (driver_location, driver_location_timestamp) = if any_location_unfiltered {
                // When few unfiltered locations are present
                (&latest_driver_location.pt, &latest_driver_location_ts)
            } else {
                // When all locations are filtered
                if let Some(driver_last_known_location) = driver_last_known_location.as_ref() {
                    (
                        &driver_last_known_location.location,
                        &driver_last_known_location.timestamp,
                    )
                } else {
                    (&latest_driver_location.pt, &latest_driver_location_ts)
                }
            };

            let set_driver_last_location_update = async {
                set_driver_last_location_update(
                    &data.redis,
                    &data.last_location_timstamp_expiry,
                    &driver_id,
                    &merchant_id,
                    driver_location,
                    driver_location_timestamp,
                    &None::<TimeStamp>,
                    stop_detection,
                    &driver_ride_status,
                    &driver_ride_notification_status,
                    &Some(detection_state),
                    &Some(anti_detection_state),
                    &Some(violation_trigger_flag),
                    &driver_pickup_distance,
                    &latest_driver_location.bear,
                    &Some(vehicle_type.clone()),
                )
                .await?;
                Ok(())
            };
            all_tasks.push(Box::pin(set_driver_last_location_update));

            if any_location_unfiltered {
                if let (Some(RideStatus::INPROGRESS), Some(ride_id)) =
                    (driver_ride_status.as_ref(), driver_ride_id.as_ref())
                {
                    let geo_entries = locations
                        .iter()
                        .filter_map(|(loc, location_type)| match location_type {
                            LocationType::UNFILTERED => Some(Point {
                                lat: loc.pt.lat,
                                lon: loc.pt.lon,
                            }),
                            LocationType::FILTERED => None,
                        })
                        .collect::<Vec<Point>>();

                    let on_ride_driver_locations_count = get_on_ride_driver_locations_count(
                        &data.redis,
                        &driver_id.clone(),
                        &merchant_id,
                    )
                    .await?;

                    if on_ride_driver_locations_count + geo_entries.len() as i64 > data.batch_size {
                        let mut on_ride_driver_locations = get_on_ride_driver_locations_and_delete(
                            &data.redis,
                            &driver_id,
                            &merchant_id,
                            on_ride_driver_locations_count,
                        )
                        .await?;
                        on_ride_driver_locations.extend(geo_entries);

                        let bulk_location_update_dobpp = async {
                            bulk_location_update_dobpp(
                                &data.bulk_location_callback_url,
                                ride_id.to_owned(),
                                driver_id.to_owned(),
                                on_ride_driver_locations,
                            )
                            .await
                            .map_err(|err| {
                                AppError::DriverBulkLocationUpdateFailed(err.message())
                            })?;
                            Ok(())
                        };
                        all_tasks.push(Box::pin(bulk_location_update_dobpp));
                    } else {
                        let push_on_ride_driver_locations = async {
                            push_on_ride_driver_locations(
                                &data.redis,
                                &driver_id,
                                &merchant_id,
                                geo_entries,
                                &data.redis_expiry,
                            )
                            .await?;
                            Ok(())
                        };
                        all_tasks.push(Box::pin(push_on_ride_driver_locations));
                    }
                }
            }

            join_all(all_tasks)
                .await
                .into_iter()
                .try_for_each(Result::from)?;

            (locations, None)
        }
    };

    Arbiter::current().spawn(async move {
        match driver_ride_info {
            Some(RideInfo::Bus {
                source,
                destination,
                ..
            }) => {
                match driver_ride_status {
                    Some(RideStatus::NEW) => {
                        if let (Some(source), Some(ride_id), Some(upcoming_stops)) =
                            (source, driver_ride_id.as_ref(), upcoming_stops.as_ref())
                        {
                            if let Some(upcoming_stop) = upcoming_stops.first() {
                                if latest_driver_location.acc.is_some_and(|Accuracy(acc)| {
                                    acc < data.driver_location_accuracy_buffer
                                }) && distance_between_in_meters(
                                    &source,
                                    &latest_driver_location.pt,
                                ) > data.driver_source_departed_buffer
                                    && upcoming_stop
                                        .distance_from_previous_intermediate_stop
                                        .inner() as f64
                                        > data.driver_source_departed_buffer
                                {
                                    let _ = driver_source_departed(
                                        &data.driver_source_departed_callback_url,
                                        &latest_driver_location.pt,
                                        ride_id.to_owned(),
                                        driver_id.to_owned(),
                                        vehicle_type.to_owned(),
                                    )
                                    .await;
                                }
                            };
                        }
                    }
                    Some(RideStatus::INPROGRESS) => {
                        if let (Some(location), Some(ride_id)) =
                            (stop_detected.as_ref(), driver_ride_id.as_ref())
                        {
                            if distance_between_in_meters(&destination, &location)
                                < data.driver_reached_destination_buffer
                            {
                                let _ = driver_reached_destination(
                                    &data.driver_reached_destination_callback_url,
                                    &location,
                                    ride_id.to_owned(),
                                    driver_id.to_owned(),
                                    vehicle_type.to_owned(),
                                )
                                .await;
                            }
                        }
                    }
                    _ => {}
                }

                for violations in violation_detection_requests {
                    if let Err(err) =
                        trigger_detection_alert(&data.detection_callback_url, violations)
                            .await
                            .map_err(|err| AppError::AlertRequestFailed(err.message()))
                    {
                        warn!("Violation Alert could not be sent. {} ", err);
                    }
                }
            }
            Some(RideInfo::Car {
                pickup_location: _,
                min_distance_between_two_points: _,
            }) => {
                if let (Some(location), Some(ride_id)) =
                    (stop_detected.as_ref(), driver_ride_id.as_ref())
                {
                    if let Some(stop_detection_config) = data.stop_detection.get(&base_vehicle_type)
                    {
                        let stop_detection_config = match driver_ride_status {
                            Some(RideStatus::NEW) => stop_detection_config.get(&RideStatus::NEW),
                            Some(RideStatus::INPROGRESS) => {
                                stop_detection_config.get(&RideStatus::INPROGRESS)
                            }
                            _ => None,
                        };
                        if let Some(config) = stop_detection_config {
                            let _ = trigger_stop_detection_event(
                                &config.stop_detection_update_callback_url,
                                location,
                                ride_id.to_owned(),
                                driver_id.to_owned(),
                            )
                            .await;
                        }
                    }
                }

                if is_driver_ride_notification_status_changed {
                    if let (Some(status), Some(driver_ride_id)) =
                        (driver_ride_notification_status, driver_ride_id.clone())
                    {
                        let _ = trigger_fcm_bap(
                            &data.trigger_fcm_callback_url_bap,
                            driver_ride_id,
                            driver_id.clone(),
                            status,
                        )
                        .await
                        .map_err(|err| AppError::DriverSendingFCMFailed(err.message()));
                    }
                }
            }
            None => {
                if let (Some(location), Some(ride_id)) =
                    (stop_detected.as_ref(), driver_ride_id.as_ref())
                {
                    if let Some(stop_detection_config) = data.stop_detection.get(&base_vehicle_type)
                    {
                        let stop_detection_config = match driver_ride_status {
                            Some(RideStatus::NEW) => stop_detection_config.get(&RideStatus::NEW),
                            Some(RideStatus::INPROGRESS) => {
                                stop_detection_config.get(&RideStatus::INPROGRESS)
                            }
                            _ => None,
                        };
                        if let Some(config) = stop_detection_config {
                            let _ = trigger_stop_detection_event(
                                &config.stop_detection_update_callback_url,
                                location,
                                ride_id.to_owned(),
                                driver_id.to_owned(),
                            )
                            .await;
                        }
                    }
                }
            }
        }

        kafka_stream_updates(
            &data.producer,
            &data.driver_location_update_topic,
            locations,
            current_ts,
            merchant_id,
            merchant_operating_city_id,
            driver_ride_id,
            driver_ride_status,
            driver_mode,
            &driver_id,
            vehicle_type,
            stop_detected,
            // travelled_distance,
        )
        .await;
    });

    Ok(())
}

#[macros::measure_duration]
pub async fn track_driver_location(
    data: Data<AppState>,
    ride_id: RideId,
) -> Result<DriverLocationResponse, AppError> {
    let RideId(unwrapped_ride_id) = ride_id.to_owned();

    let driver_details =
        get_driver_details(&data.redis, &ride_id)
            .await?
            .ok_or(AppError::InvalidRideStatus(
                unwrapped_ride_id.to_owned(),
                "COMPLETED".to_string(),
            ))?;

    let driver_location_details = get_driver_location(&data.redis, &driver_details.driver_id)
        .await?
        .ok_or(AppError::DriverLastKnownLocationNotFound)?;

    let delay_time = match driver_location_details.ride_status {
        Some(RideStatus::NEW) => {
            Utc::now().timestamp() - data.driver_location_delay_for_new_ride_sec
        }
        _ => Utc::now().timestamp() - data.driver_location_delay_in_sec,
    };

    let TimeStamp(driver_update_time) =
        driver_location_details.driver_last_known_location.timestamp;
    if driver_update_time.timestamp() < delay_time {
        Arbiter::current().spawn(async move {
            let _ = trigger_fcm_dobpp(
                &data.trigger_fcm_callback_url,
                ride_id,
                driver_details.driver_id,
            )
            .await
            .map_err(|err| AppError::DriverSendingFCMFailed(err.message()));
        });
    }

    Ok(DriverLocationResponse {
        curr_point: driver_location_details.driver_last_known_location.location,
        last_update: driver_location_details.driver_last_known_location.timestamp,
    })
}
