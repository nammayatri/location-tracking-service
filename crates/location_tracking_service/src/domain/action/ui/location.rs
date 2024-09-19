/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use std::pin::Pin;

use crate::common::utils::{distance_between_in_meters, get_city, is_blacklist_for_special_zone};
use crate::common::{sliding_window_rate_limiter::sliding_window_limiter, types::*};
use crate::domain::types::ui::location::*;
use crate::environment::AppState;
use crate::kafka::producers::kafka_stream_updates;
use crate::outbound::external::{
    authenticate_dobpp, bulk_location_update_dobpp, trigger_fcm_dobpp,
};
use crate::redis::{commands::*, keys::*};
use crate::tools::error::AppError;
use actix::Arbiter;
use actix_web::{web::Data, HttpResponse};
use chrono::Utc;
use futures::future::join_all;
use futures::Future;
use reqwest::Url;
use shared::redis::types::RedisConnectionPool;
use tracing::{info, warn};

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
) -> Vec<UpdateDriverLocationRequest> {
    locations.dedup_by(|a, b| a.pt.lat == b.pt.lat && a.pt.lon == b.pt.lon);

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .into_iter()
        .filter(|location| {
            location.acc.or(Some(Accuracy(0.0))) <= Some(min_location_accuracy)
                && last_known_location
                    .map(|last_known_location| {
                        distance_between_in_meters(&last_known_location.location, &location.pt)
                            > driver_location_accuracy_buffer
                    })
                    .unwrap_or(true)
        })
        .collect();

    locations
}

#[macros::measure_duration]
pub async fn update_driver_location(
    token: Token,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    mut locations: Vec<UpdateDriverLocationRequest>,
    driver_mode: DriverMode,
) -> Result<HttpResponse, AppError> {
    let (driver_id, merchant_id, merchant_operating_city_id) = get_driver_id_from_authentication(
        &data.redis,
        &data.auth_url,
        &data.auth_api_key,
        &data.auth_token_expiry,
        &token,
    )
    .await?;

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

    let last_known_location = get_driver_location(&data.redis, &driver_id).await?;

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .into_iter()
        .filter(|location| {
            last_known_location
                .as_ref()
                .map(|(last_known_location, _)| location.ts > last_known_location.timestamp)
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
            last_known_location,
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

#[allow(clippy::type_complexity)]
async fn process_driver_locations(
    args: (
        Data<AppState>,
        Vec<UpdateDriverLocationRequest>,
        UpdateDriverLocationRequest,
        Option<(DriverLastKnownLocation, Option<Meters>)>,
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
        last_known_location,
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

    let TimeStamp(latest_driver_location_ts) = latest_driver_location.ts;
    let current_ts = Utc::now();
    let latest_driver_location_ts = if latest_driver_location_ts > current_ts {
        warn!(
            "Latest driver location timestamp in future => {}, Switching to current time => {}",
            latest_driver_location_ts, current_ts
        );
        TimeStamp(current_ts)
    } else {
        TimeStamp(latest_driver_location_ts)
    };

    let mut all_tasks: Vec<Pin<Box<dyn Future<Output = Result<(), AppError>>>>> = Vec::new();

    // let travelled_distance = if driver_ride_status == Some(RideStatus::NEW)
    //     || driver_ride_status == Some(RideStatus::INPROGRESS)
    // {
    //     Meters(
    //         last_known_location
    //             .to_owned()
    //             .map(|(last_known_location, travelled_distance)| {
    //                 travelled_distance
    //                     .map(|travelled_distance| travelled_distance.inner())
    //                     .unwrap_or(0)
    //                     + distance_between_in_meters(
    //                         &last_known_location.location,
    //                         &latest_driver_location.pt,
    //                     )
    //                     .floor() as u32
    //             })
    //             .unwrap_or(0),
    //     )
    // } else {
    //     Meters(0)
    // };

    let set_driver_last_location_update = async {
        set_driver_last_location_update(
            &data.redis,
            &data.last_location_timstamp_expiry,
            &driver_id,
            &merchant_id,
            &latest_driver_location.pt,
            &latest_driver_location_ts,
            // travelled_distance.to_owned(),
        )
        .await?;
        Ok(())
    };
    all_tasks.push(Box::pin(set_driver_last_location_update));

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

    let locations = if let Some(RideStatus::INPROGRESS) = driver_ride_status.as_ref() {
        let locations = get_filtered_driver_locations(
            last_known_location
                .map(|(last_known_location, _)| last_known_location)
                .as_ref(),
            locations,
            data.min_location_accuracy,
            data.driver_location_accuracy_buffer,
        );
        if !locations.is_empty() {
            if locations.len() > data.batch_size as usize {
                warn!(
                    "On Ride Way points more than {} points after filtering => {} points",
                    data.batch_size,
                    locations.len()
                );
            }
            locations
        } else {
            join_all(all_tasks)
                .await
                .into_iter()
                .try_for_each(Result::from)?;
            return Ok(());
        }
    } else {
        locations
    };

    if let (Some(RideStatus::INPROGRESS), Some(ride_id)) =
        (driver_ride_status.as_ref(), driver_ride_id.as_ref())
    {
        let geo_entries = locations
            .iter()
            .map(|loc| Point {
                lat: loc.pt.lat,
                lon: loc.pt.lon,
            })
            .collect::<Vec<Point>>();

        let on_ride_driver_locations_count =
            get_on_ride_driver_locations_count(&data.redis, &driver_id.clone(), &merchant_id)
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
                .map_err(|err| AppError::DriverBulkLocationUpdateFailed(err.message()))?;
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

    join_all(all_tasks)
        .await
        .into_iter()
        .try_for_each(Result::from)?;

    Arbiter::current().spawn(async move {
        // if driver_ride_status == Some(RideStatus::NEW) {
        //     if let Some(driver_ride_details) = driver_ride_details {
        //         if let (
        //             Some(vehicle_number),
        //             Some(ride_start_otp),
        //             Some(estimated_pickup_distance),
        //         ) = (
        //             driver_ride_details.vehicle_number,
        //             driver_ride_details.ride_start_otp,
        //             driver_ride_details.estimated_pickup_distance,
        //         ) {
        //             let _ = trigger_liveactivity(
        //                 &data.apns_url,
        //                 vehicle_type.to_owned(),
        //                 vehicle_number,
        //                 ride_start_otp,
        //                 estimated_pickup_distance,
        //                 travelled_distance,
        //             )
        //             .await;
        //         }
        //     }
        // }
        kafka_stream_updates(
            &data.producer,
            &data.driver_location_update_topic,
            locations,
            merchant_id,
            merchant_operating_city_id,
            driver_ride_id,
            driver_ride_status,
            driver_mode,
            &driver_id,
            vehicle_type,
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

    let (driver_last_known_location_details, _) =
        get_driver_location(&data.redis, &driver_details.driver_id)
            .await?
            .ok_or(AppError::DriverLastKnownLocationNotFound)?;

    let delay_time = Utc::now().timestamp() - data.driver_location_delay_in_sec;
    let TimeStamp(driver_update_time) = driver_last_known_location_details.timestamp;
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
        curr_point: driver_last_known_location_details.location,
        last_update: driver_last_known_location_details.timestamp,
    })
}
