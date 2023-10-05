/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
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
use rustc_hash::FxHashSet;
use shared::redis::types::RedisConnectionPool;
use shared::tools::error::AppError;
use shared::utils::logger::*;
use std::f64::consts::PI;

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
        return Ok(response.driver_id);
    }

    Err(AppError::DriverAppAuthFailed(token))
}

fn deg2rad(degrees: f64) -> f64 {
    degrees * PI / 180.0
}

fn distance_between_in_meters(latlong1: &Point, latlong2: &Point) -> f64 {
    // Radius of Earth in meters
    let r: f64 = 6_371_000.0;

    let Latitude(lat1) = latlong1.lat;
    let Longitude(lon1) = latlong1.lon;
    let Latitude(lat2) = latlong2.lat;
    let Longitude(lon2) = latlong2.lon;

    let dlat = deg2rad(lat2 - lat1);
    let dlon = deg2rad(lon2 - lon1);

    let rlat1 = deg2rad(lat1);
    let rlat2 = deg2rad(lat2);

    let sq = |x: f64| x * x;

    // Calculated distance is real (not imaginary) when 0 <= h <= 1
    // Ideally in our use case h wouldn't go out of bounds
    let h = sq((dlat / 2.0).sin()) + rlat1.cos() * rlat2.cos() * sq((dlon / 2.0).sin());

    2.0 * r * h.sqrt().atan2((1.0 - h).sqrt())
}

pub async fn update_driver_location(
    token: Token,
    merchant_id: MerchantId,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    mut locations: Vec<UpdateDriverLocationRequest>,
    driver_mode: Option<DriverMode>,
) -> Result<APISuccess, AppError> {
    locations.sort_by(|a, b| {
        let TimeStamp(a_ts) = a.ts;
        let TimeStamp(b_ts) = b.ts;
        (a_ts).cmp(&b_ts)
    });

    let mut unique_locations: FxHashSet<String> = FxHashSet::default();
    let mut new_locations: Vec<UpdateDriverLocationRequest> = Vec::new();
    for location in locations.iter() {
        let x = serde_json::to_string(location).unwrap();
        if !(unique_locations.contains(&x)) {
            unique_locations.insert(x);
            new_locations.push(location.clone());
        }
    }

    let locations: Vec<UpdateDriverLocationRequest> = new_locations;

    let latest_driver_location = if let Some(locations) = locations.last() {
        locations.to_owned()
    } else {
        return Ok(APISuccess::default());
    };

    let city = get_city(
        &latest_driver_location.pt.lat,
        &latest_driver_location.pt.lon,
        &data.polygon,
    )?;

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

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .into_iter()
        .filter(|request| request.acc.or(Some(Accuracy(0.0))) <= Some(data.min_location_accuracy))
        .collect();

    let driver_last_known_location_details =
        get_driver_location(&data.persistent_redis, &driver_id).await?;

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .into_iter()
        .filter(|request| {
            let request_location = &request.pt;
            let distance = distance_between_in_meters(
                request_location,
                &driver_last_known_location_details.location,
            );
            distance > data.driver_location_accuracy_buffer
        })
        .collect();

    info!(
        tag = "[Location Updates]",
        "Got location updates for Driver Id : {:?} : {:?}", &driver_id, &locations
    );

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
        driver_id,
        merchant_id,
        vehicle_type,
        city,
        driver_mode,
    ) = args;

    let last_location_update_ts =
        get_driver_last_location_update(&data.persistent_redis, &driver_id)
            .await
            .unwrap_or(latest_driver_location.ts);

    let (
        t_persistent_redis,
        t_last_location_timstamp_expiry,
        t_driver_id,
        t_merchant_id,
        t_driver_mode,
    ) = (
        data.persistent_redis.clone(),
        data.last_location_timstamp_expiry,
        driver_id.to_owned(),
        merchant_id.to_owned(),
        driver_mode.to_owned(),
    );

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .into_iter()
        .filter(|request| request.ts >= last_location_update_ts)
        .collect();

    let latest_driver_location = if let Some(location) = locations.last() {
        location.to_owned()
    } else {
        return;
    };

    Arbiter::current().spawn(async move {
        let _ = set_driver_last_location_update(
            &t_persistent_redis,
            &t_last_location_timstamp_expiry,
            &t_driver_id,
            &t_merchant_id,
            &Point {
                lat: latest_driver_location.pt.lat,
                lon: latest_driver_location.pt.lon,
            },
            t_driver_mode,
        )
        .await;
    });

    let driver_ride_details =
        get_ride_details(&data.persistent_redis, &driver_id, &merchant_id).await;

    match driver_ride_details {
        Ok(RideDetails {
            ride_id,
            ride_status: RideStatus::INPROGRESS,
            ..
        }) => {
            let _ = &data
                .sender
                .send((
                    Dimensions {
                        merchant_id: merchant_id.to_owned(),
                        city: city.to_owned(),
                        vehicle_type: vehicle_type.to_owned(),
                        new_ride: false,
                    },
                    latest_driver_location.pt.lat,
                    latest_driver_location.pt.lon,
                    driver_id.to_owned(),
                ))
                .await;

            if locations.len() > 100 {
                error!(
                    "Way points more than 100 points {} on_ride: True",
                    locations.len()
                );
            }

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

            if let Ok(on_ride_driver_location_count) = on_ride_driver_location_count {
                if on_ride_driver_location_count >= data.batch_size {
                    let on_ride_driver_locations = get_on_ride_driver_locations(
                        &data.persistent_redis,
                        &driver_id,
                        &merchant_id,
                        on_ride_driver_location_count,
                    )
                    .await;

                    if let Ok(on_ride_driver_locations) = on_ride_driver_locations {
                        let res = bulk_location_update_dobpp(
                            &data.bulk_location_callback_url,
                            ride_id.to_owned(),
                            driver_id.to_owned(),
                            on_ride_driver_locations.to_owned(),
                        )
                        .await;

                        if res.is_err() {
                            let _ = push_on_ride_driver_locations(
                                &data.persistent_redis,
                                &driver_id,
                                &merchant_id,
                                &on_ride_driver_locations,
                                &data.redis_expiry,
                            )
                            .await;
                        }
                    }
                }
            }

            Arbiter::current().spawn(async move {
                let _ = kafka_stream_updates(
                    &data.producer,
                    &data.driver_location_update_topic,
                    locations,
                    merchant_id,
                    Some(ride_id),
                    Some(RideStatus::INPROGRESS),
                    driver_mode,
                    &driver_id,
                )
                .await;
            });
        }
        Ok(RideDetails {
            ride_id,
            ride_status: RideStatus::NEW,
            ..
        }) => {
            let _ = &data
                .sender
                .send((
                    Dimensions {
                        merchant_id: merchant_id.to_owned(),
                        city: city.to_owned(),
                        vehicle_type: vehicle_type.to_owned(),
                        new_ride: true,
                    },
                    latest_driver_location.pt.lat,
                    latest_driver_location.pt.lon,
                    driver_id.to_owned(),
                ))
                .await;

            if locations.len() > 100 {
                error!(
                    "Way points more than 100 points {} on_ride: False",
                    locations.len()
                );
            }

            Arbiter::current().spawn(async move {
                let _ = kafka_stream_updates(
                    &data.producer,
                    &data.driver_location_update_topic,
                    locations,
                    merchant_id,
                    Some(ride_id),
                    Some(RideStatus::NEW),
                    driver_mode,
                    &driver_id,
                )
                .await;
            });
        }
        _ => {
            let _ = &data
                .sender
                .send((
                    Dimensions {
                        merchant_id: merchant_id.to_owned(),
                        city: city.to_owned(),
                        vehicle_type: vehicle_type.to_owned(),
                        new_ride: false,
                    },
                    latest_driver_location.pt.lat,
                    latest_driver_location.pt.lon,
                    driver_id.to_owned(),
                ))
                .await;

            if locations.len() > 100 {
                error!(
                    "Way points more than 100 points {} on_ride: False",
                    locations.len()
                );
            }

            Arbiter::current().spawn(async move {
                let _ = kafka_stream_updates(
                    &data.producer,
                    &data.driver_location_update_topic,
                    locations,
                    merchant_id,
                    None,
                    None,
                    driver_mode,
                    &driver_id,
                )
                .await;
            });
        }
    }
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
