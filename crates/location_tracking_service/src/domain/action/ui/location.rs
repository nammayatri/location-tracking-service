/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::{
    kafka::push_to_kafka, sliding_window_rate_limiter::sliding_window_limiter, types::*,
    utils::get_city,
};
use crate::domain::types::ui::location::*;
use crate::environment::AppState;
use crate::redis::{commands::*, keys::*};
use actix::Arbiter;
use actix_web::web::Data;
use chrono::Utc;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use shared::tools::error::AppError;
use shared::utils::callapi::*;
use shared::utils::logger::*;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseData {
    #[serde(rename = "driverId")]
    pub driver_id: DriverId,
}

async fn kafka_stream_updates(
    data: Data<AppState>,
    merchant_id: MerchantId,
    ride_id: String,
    locations: Vec<UpdateDriverLocationRequest>,
    ride_status: Option<RideStatus>,
    driver_mode: Option<DriverMode>,
    DriverId(driver_id): &DriverId,
) {
    let topic = &data.driver_location_update_topic;

    let ride_status = match ride_status {
        Some(ride_status) => ride_status.to_string(),
        None => "".to_string(),
    };

    let driver_mode = match driver_mode {
        Some(driver_mode) => driver_mode.to_string(),
        None => "".to_string(),
    };

    for loc in locations {
        let message = LocationUpdate {
            r_id: ride_id.clone(),
            m_id: merchant_id.clone(),
            ts: loc.ts,
            st: TimeStamp(Utc::now()),
            pt: Point {
                lat: loc.pt.lat,
                lon: loc.pt.lon,
            },
            acc: loc.acc.unwrap_or(Accuracy(0.0)),
            ride_status: ride_status.clone(),
            da: true,
            mode: driver_mode.clone(),
        };
        push_to_kafka(&data.producer, topic, driver_id.as_str(), message).await;
    }
}

async fn get_driver_id_from_authentication(
    data: Data<AppState>,
    wrapped_token @ Token(token): &Token,
    MerchantId(merchant_id): &MerchantId,
) -> Result<Option<DriverId>, AppError> {
    let response = call_api::<AuthResponseData, String>(
        Method::GET,
        &data.auth_url,
        vec![
            ("content-type", "application/json"),
            ("token", token),
            ("api-key", &data.auth_api_key),
            ("merchant-id", merchant_id),
        ],
        None,
    )
    .await;

    if let Ok(response) = response {
        set_driver_id(
            &data.persistent_redis,
            &data.auth_token_expiry,
            wrapped_token,
            &response.driver_id,
        )
        .await?;
        return Ok(Some(response.driver_id));
    }

    Err(AppError::DriverAppAuthFailed)
}

pub async fn update_driver_location(
    token: Token,
    merchant_id: MerchantId,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    request_body: Vec<UpdateDriverLocationRequest>,
    driver_mode: Option<DriverMode>,
) -> Result<APISuccess, AppError> {
    let city = get_city(
        request_body[0].pt.lat,
        request_body[0].pt.lon,
        data.polygon.clone(),
    )?;

    let driver_id = match get_driver_id(&data.persistent_redis, &token).await? {
        Some(driver_id) => Some(driver_id),
        None => get_driver_id_from_authentication(data.clone(), &token, &merchant_id).await?,
    };

    if let Some(DriverId(driver_id)) = driver_id {
        info!(
            tag = "[Location Updates]",
            "Got location updates for Driver Id : {} : {:?}", &driver_id, &request_body
        );

        let _ = sliding_window_limiter(
            &data.persistent_redis,
            &sliding_rate_limiter_key(&DriverId(driver_id.clone()), &city, &merchant_id),
            data.location_update_limit,
            data.location_update_interval as u32,
        )
        .await;

        with_lock_redis(
            &data.persistent_redis,
            driver_processing_location_update_lock_key(
                &DriverId(driver_id.clone()),
                &merchant_id,
                &city.clone(),
            ),
            60,
            process_driver_locations,
            (
                data.clone(),
                request_body.clone(),
                DriverId(driver_id.clone()),
                merchant_id.clone(),
                vehicle_type.clone(),
                city.clone(),
                driver_mode.clone(),
            ),
        )
        .await?;

        Ok(APISuccess::default())
    } else {
        Err(AppError::InternalError(
            "Failed to authenticate and get driver_id".to_string(),
        ))
    }
}

async fn process_driver_locations(
    args: (
        Data<AppState>,
        Vec<UpdateDriverLocationRequest>,
        DriverId,
        MerchantId,
        VehicleType,
        CityName,
        Option<DriverMode>,
    ),
) {
    let (data, mut locations, driver_id, merchant_id, vehicle_type, city, driver_mode) = args;

    locations.sort_by(|a, b| {
        let TimeStamp(a_ts) = a.ts;
        let TimeStamp(b_ts) = b.ts;
        (a_ts).cmp(&b_ts)
    });

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .clone()
        .into_iter()
        .filter(|request| request.acc.or(Some(Accuracy(0.0))) <= Some(data.min_location_accuracy))
        .collect();

    let driver_location = if locations.is_empty() {
        return;
    } else {
        &locations[locations.len() - 1]
    };

    let last_location_update_ts =
        get_driver_last_location_update(&data.persistent_redis, &driver_id)
            .await
            .unwrap_or(driver_location.ts);

    let driver_location = Point {
        lat: driver_location.pt.lat,
        lon: driver_location.pt.lon,
    };

    let (t_data, t_driver_id, t_merchant_id, t_driver_location, t_driver_mode) = (
        data.clone(),
        driver_id.clone(),
        merchant_id.clone(),
        driver_location.clone(),
        driver_mode.clone(),
    );

    Arbiter::current().spawn(async move {
        let _ = set_driver_last_location_update(
            &t_data.persistent_redis,
            &t_data.last_location_timstamp_expiry,
            &t_driver_id,
            &t_merchant_id,
            &t_driver_location,
            t_driver_mode,
        )
        .await;
    });

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .clone()
        .into_iter()
        .filter(|request| request.ts >= last_location_update_ts)
        .collect();

    let loc = if locations.is_empty() {
        return;
    } else {
        locations[locations.len() - 1].clone()
    };

    let driver_ride_details =
        get_ride_details(&data.persistent_redis, &driver_id, &merchant_id).await;

    let _ = &data
        .sender
        .send((
            Dimensions {
                merchant_id: merchant_id.clone(),
                city: city.clone(),
                vehicle_type: vehicle_type.clone(),
            },
            loc.pt.lat,
            loc.pt.lon,
            driver_id.clone(),
        ))
        .await;

    if let Ok(Some(RideDetails {
        ride_id,
        ride_status: RideStatus::INPROGRESS,
        ..
    })) = driver_ride_details
    {
        process_on_ride_driver_location(
            data.clone(),
            merchant_id,
            ride_id,
            driver_id,
            driver_mode,
            locations,
        )
        .await;
    } else {
        if locations.len() > 100 {
            error!(
                "Way points more than 100 points {} on_ride: False",
                locations.len()
            );
        }

        Arbiter::current().spawn(async move {
            let _ = kafka_stream_updates(
                data,
                merchant_id,
                "".to_string(),
                locations,
                None,
                driver_mode,
                &driver_id,
            )
            .await;
        });
    }
}

async fn process_on_ride_driver_location(
    data: Data<AppState>,
    merchant_id: MerchantId,
    ride_id: RideId,
    driver_id: DriverId,
    driver_mode: Option<DriverMode>,
    locations: Vec<UpdateDriverLocationRequest>,
) {
    if locations.len() > 100 {
        error!(
            "Way points more than 100 points {} on_ride: True",
            locations.len()
        );
    }

    let (t_merchant_id, t_data, RideId(t_ride_id), t_locations, t_driver_id) = (
        merchant_id.clone(),
        data.clone(),
        ride_id.clone(),
        locations.clone(),
        driver_id.clone(),
    );
    Arbiter::current().spawn(async move {
        let _ = kafka_stream_updates(
            t_data,
            t_merchant_id,
            t_ride_id,
            t_locations,
            Some(RideStatus::INPROGRESS),
            driver_mode,
            &t_driver_id,
        )
        .await;
    });

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
                let res = call_api::<APISuccess, BulkDataReq>(
                    Method::POST,
                    &data.bulk_location_callback_url,
                    vec![("content-type", "application/json")],
                    Some(BulkDataReq {
                        ride_id: ride_id.clone(),
                        driver_id: driver_id.clone(),
                        loc: on_ride_driver_locations.clone(),
                    }),
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
}

pub async fn track_driver_location(
    data: Data<AppState>,
    ride_id: RideId,
) -> Result<DriverLocationResponse, AppError> {
    let driver_details = get_driver_details(&data.persistent_redis, &ride_id).await?;

    let driver_ride_details = get_ride_details(
        &data.persistent_redis,
        &driver_details.driver_id,
        &driver_details.merchant_id,
    )
    .await?;

    match driver_ride_details {
        Some(driver_ride_details) => {
            let current_ride_status = match driver_ride_details.ride_status {
                RideStatus::NEW => DriverRideStatus::PreRide,
                RideStatus::INPROGRESS => DriverRideStatus::ActualRide,
                _ => {
                    return Err(AppError::InvalidRequest(
                        "Driver Ride Status is Invalid".to_string(),
                    ))
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
        None => Err(AppError::InvalidRequest(
            "Driver Ride Details not found.".to_string(),
        )),
    }
}
