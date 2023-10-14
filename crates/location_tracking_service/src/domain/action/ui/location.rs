/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::utils::distance_between_in_meters;
use crate::common::{
    sliding_window_rate_limiter::sliding_window_limiter, types::*, utils::get_city,
};
use crate::domain::types::ui::location::*;
use crate::environment::AppState;
use crate::kafka::producers::kafka_stream_updates;
use crate::outbound::external::{authenticate_dobpp, bulk_location_update_dobpp};
use crate::redis::{commands::*, keys::*};
use actix::Arbiter;
use actix_web::web::Data;
use chrono::Utc;
use shared::redis::types::RedisConnectionPool;
use shared::tools::error::AppError;
use shared::utils::logger::*;

async fn get_driver_id_from_authentication(
    persistent_redis: &RedisConnectionPool,
    auth_url: &str,
    auth_api_key: &str,
    auth_token_expiry: &u32,
    Token(token): Token,
    MerchantId(merchant_id): &MerchantId,
) -> Result<DriverId, AppError> {
    let response = authenticate_dobpp(auth_url, token.as_str(), auth_api_key, merchant_id).await;

    if let Ok(response) = response {
        set_driver_id(
            persistent_redis,
            auth_token_expiry,
            &Token(token),
            &response.driver_id,
        )
        .await?;
        Ok(response.driver_id)
    } else {
        Err(AppError::DriverAppAuthFailed(token))
    }
}

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

pub async fn update_driver_location(
    token: Token,
    merchant_id: MerchantId,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    mut locations: Vec<UpdateDriverLocationRequest>,
    driver_mode: Option<DriverMode>,
) -> Result<APISuccess, AppError> {
    let driver_id = match get_driver_id(&data.persistent_redis, &token).await? {
        Some(driver_id) => driver_id,
        None => {
            get_driver_id_from_authentication(
                &data.persistent_redis,
                &data.auth_url,
                &data.auth_api_key,
                &data.auth_token_expiry,
                token,
                &merchant_id,
            )
            .await?
        }
    };

    if locations.len() > 100 {
        warn!(
            "Way points more than 100 points => {} points",
            locations.len()
        );
    }

    locations.sort_by(|a, b| {
        let TimeStamp(a_ts) = a.ts;
        let TimeStamp(b_ts) = b.ts;
        (a_ts).cmp(&b_ts)
    });

    let last_known_location = get_driver_location(&data.persistent_redis, &driver_id)
        .await
        .ok();

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .into_iter()
        .filter(|location| {
            last_known_location
                .as_ref()
                .map(|last_known_location| location.ts > last_known_location.timestamp)
                .unwrap_or(true)
        })
        .collect();

    let latest_driver_location = if let Some(location) = locations.last() {
        location.to_owned()
    } else {
        return Ok(APISuccess::default());
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

    let _ = sliding_window_limiter(
        &data.persistent_redis,
        &sliding_rate_limiter_key(&driver_id, &city, &merchant_id),
        data.location_update_limit,
        data.location_update_interval as u32,
    )
    .await;

    with_lock_redis(
        &data.persistent_redis,
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
            vehicle_type,
            city,
            driver_mode,
        ),
    )
    .await?;

    Ok(APISuccess::default())
}

#[allow(clippy::type_complexity)]
async fn process_driver_locations(
    args: (
        Data<AppState>,
        Vec<UpdateDriverLocationRequest>,
        UpdateDriverLocationRequest,
        Option<DriverLastKnownLocation>,
        DriverId,
        MerchantId,
        VehicleType,
        CityName,
        Option<DriverMode>,
    ),
) {
    let (
        data,
        locations,
        latest_driver_location,
        last_known_location,
        driver_id,
        merchant_id,
        vehicle_type,
        city,
        driver_mode,
    ) = args;

    let driver_ride_details =
        get_ride_details(&data.persistent_redis, &driver_id, &merchant_id).await;

    let driver_ride_id = driver_ride_details
        .as_ref()
        .ok()
        .map(|ride_details| ride_details.ride_id.to_owned());

    let driver_ride_status = driver_ride_details
        .as_ref()
        .ok()
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

    let driver_mode = match set_driver_last_location_update(
        &data.persistent_redis,
        &data.last_location_timstamp_expiry,
        &driver_id,
        &merchant_id,
        &latest_driver_location.pt,
        &latest_driver_location_ts,
        driver_mode.to_owned(),
    )
    .await
    {
        Ok(driver_details) => driver_details.driver_mode,
        Err(err) => {
            error!(
                "Error occured in set_driver_last_location_update => {}",
                err
            );
            driver_mode
        }
    };

    let _ = &data
        .sender
        .send((
            Dimensions {
                merchant_id: merchant_id.to_owned(),
                city: city.to_owned(),
                vehicle_type: vehicle_type.to_owned(),
                new_ride: driver_ride_status
                    .as_ref()
                    .map(|ride_status| ride_status == &RideStatus::NEW)
                    .unwrap_or(false),
            },
            latest_driver_location.pt.lat,
            latest_driver_location.pt.lon,
            latest_driver_location_ts.to_owned(),
            driver_id.to_owned(),
        ))
        .await;

    let locations = if let Some(RideStatus::INPROGRESS) = driver_ride_status.as_ref() {
        let locations = get_filtered_driver_locations(
            last_known_location.as_ref(),
            locations,
            data.min_location_accuracy,
            data.driver_location_accuracy_buffer,
        );
        if !locations.is_empty() {
            locations
        } else {
            return;
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

        let on_ride_driver_location_count = push_on_ride_driver_locations(
            &data.persistent_redis,
            &driver_id.clone(),
            &merchant_id,
            &geo_entries,
            &data.redis_expiry,
        )
        .await;

        match on_ride_driver_location_count {
            Ok(on_ride_driver_location_count) => {
                if on_ride_driver_location_count >= data.batch_size {
                    let on_ride_driver_locations = get_on_ride_driver_locations(
                        &data.persistent_redis,
                        &driver_id,
                        &merchant_id,
                        on_ride_driver_location_count,
                    )
                    .await;

                    match on_ride_driver_locations {
                        Ok(on_ride_driver_locations) => {
                            let res = bulk_location_update_dobpp(
                                &data.bulk_location_callback_url,
                                ride_id.to_owned(),
                                driver_id.to_owned(),
                                on_ride_driver_locations.to_owned(),
                            )
                            .await;

                            if let Err(err) = res {
                                let _ = push_on_ride_driver_locations(
                                    &data.persistent_redis,
                                    &driver_id,
                                    &merchant_id,
                                    &on_ride_driver_locations,
                                    &data.redis_expiry,
                                )
                                .await;
                                warn!("Error occurred during bulk_location_update_dobpp, Safely adding all locations back to Redis => {} : {:?}", err, on_ride_driver_locations);
                            }
                        }
                        Err(err) => {
                            error!(
                                "Error occurred during get_on_ride_driver_locations => {}",
                                err
                            );
                        }
                    }
                }
            }
            Err(err) => {
                error!("Error occured during on_ride_driver_location_count {}", err);
            }
        }
    }

    Arbiter::current().spawn(async move {
        kafka_stream_updates(
            &data.producer,
            &data.driver_location_update_topic,
            locations,
            merchant_id,
            driver_ride_id,
            driver_ride_status,
            driver_mode,
            &driver_id,
        )
        .await;
    });
}

pub async fn track_driver_location(
    data: Data<AppState>,
    ride_id: RideId,
) -> Result<DriverLocationResponse, AppError> {
    let RideId(unwrapped_ride_id) = ride_id.to_owned();

    let driver_details = get_driver_details(&data.persistent_redis, &ride_id)
        .await
        .map_err(|_| {
            AppError::InvalidRideStatus(unwrapped_ride_id.to_owned(), "COMPLETED".to_string())
        })?;

    let driver_ride_details = get_ride_details(
        &data.persistent_redis,
        &driver_details.driver_id,
        &driver_details.merchant_id,
    )
    .await
    .map_err(|_| {
        AppError::InvalidRideStatus(unwrapped_ride_id.to_owned(), "COMPLETED".to_string())
    })?;

    let current_ride_status = match driver_ride_details.ride_status {
        RideStatus::NEW => DriverRideStatus::PreRide,
        RideStatus::INPROGRESS => DriverRideStatus::ActualRide,
        RideStatus::CANCELLED => {
            return Err(AppError::InvalidRideStatus(
                unwrapped_ride_id.to_owned(),
                "CANCELLED".to_string(),
            ));
        }
    };

    let driver_last_known_location_details =
        get_driver_location(&data.persistent_redis, &driver_details.driver_id).await?;

    Ok(DriverLocationResponse {
        curr_point: driver_last_known_location_details.location,
        total_distance: 0.0, // Backward Compatibility : To be removed
        status: current_ride_status,
        last_update: driver_last_known_location_details.timestamp,
    })
}
