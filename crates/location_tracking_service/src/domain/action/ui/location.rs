/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::{kafka::push_to_kafka, types::*, utils::get_city};
use crate::domain::types::ui::location::*;
use crate::environment::AppState;
use crate::redis::{commands::*, keys::*};
// use actix::Arbiter;
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
    pub driver_id: String,
}

async fn kafka_stream_updates(
    data: Data<AppState>,
    merchant_id: &String,
    ride_id: &String,
    loc: UpdateDriverLocationRequest,
    ride_status: Option<RideStatus>,
    driver_mode: Option<DriverMode>,
) {
    let topic = &data.driver_location_update_topic;
    let key = &data.driver_location_update_key;

    let ride_status = match ride_status {
        Some(ride_status) => ride_status.to_string(),
        None => "".to_string(),
    };

    let driver_mode = match driver_mode {
        Some(driver_mode) => driver_mode.to_string(),
        None => "".to_string(),
    };

    let loc = LocationUpdate {
        r_id: ride_id.to_string(),
        m_id: merchant_id.to_string(),
        ts: loc.ts,
        st: Utc::now(),
        pt: Point {
            lat: loc.pt.lat,
            lon: loc.pt.lon,
        },
        acc: loc.acc.unwrap_or(0),
        ride_status: ride_status.to_string(),
        da: true,
        mode: driver_mode.to_string(),
    };

    push_to_kafka(&data.producer, topic, key, loc).await;
}

async fn get_driver_id_from_authentication(
    data: Data<AppState>,
    token: &Token,
    merchant_id: &MerchantId,
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
    .await?;

    set_driver_id(data.clone(), token, &response.driver_id).await?;

    Ok(Some(response.driver_id))
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

    let driver_id = match get_driver_id(data.clone(), &token).await? {
        Some(driver_id) => Some(driver_id),
        None => get_driver_id_from_authentication(data.clone(), &token, &merchant_id).await?,
    };

    if let Some(driver_id) = driver_id {
        info!(
            tag = "[Location Updates]",
            "Got location updates for Driver Id : {driver_id} : {:?}", &request_body
        );

        let _ = data
            .sliding_window_limiter(
                &driver_id,
                data.location_update_limit,
                data.location_update_interval as u32,
                &data.persistent_redis,
            )
            .await;

        let _ = with_lock_redis(
            data.persistent_redis.clone(),
            driver_processing_location_update_lock_key(&merchant_id.clone(), &city.clone()),
            60,
            process_driver_locations,
            (
                data.clone(),
                request_body.clone(),
                driver_id.clone(),
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
) -> () {
    let (data, mut locations, driver_id, merchant_id, vehicle_type, city, driver_mode) = args;

    locations.sort_by(|a, b| (a.ts).cmp(&b.ts));

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .clone()
        .into_iter()
        .filter(|request| request.acc.or(Some(0)) <= Some(data.min_location_accuracy))
        .collect();

    let last_location_update_ts = get_driver_last_location_update(data.clone(), &driver_id)
        .await
        .unwrap_or(locations[0].ts);

    let driver_location = &locations[locations.len() - 1];
    let driver_location = Point {
        lat: driver_location.pt.lat,
        lon: driver_location.pt.lon,
    };

    let _ = set_driver_last_location_update(
        data.clone(),
        &driver_id,
        &merchant_id,
        &driver_location,
        driver_mode.clone(),
    )
    .await;

    let locations: Vec<UpdateDriverLocationRequest> = locations
        .clone()
        .into_iter()
        .filter(|request| request.ts >= last_location_update_ts)
        .collect();

    let driver_ride_details =
        get_driver_ride_details(data.clone(), &driver_id, &merchant_id, &city).await;

    if let Ok(Some(RideDetails {
        ride_id,
        ride_status,
    })) = driver_ride_details
    {
        if ride_status == RideStatus::INPROGRESS {
            process_on_ride_driver_location(
                data.clone(),
                merchant_id,
                city,
                vehicle_type,
                ride_id,
                driver_id,
                driver_mode,
                RideStatus::INPROGRESS,
                locations,
            )
            .await;
        } else {
            process_off_ride_driver_location(
                data.clone(),
                merchant_id,
                city,
                vehicle_type,
                driver_id,
                driver_mode,
                None,
                locations,
            )
            .await;
        }
    } else {
        process_off_ride_driver_location(
            data.clone(),
            merchant_id,
            city,
            vehicle_type,
            driver_id,
            driver_mode,
            None,
            locations,
        )
        .await;
    }
}

async fn process_on_ride_driver_location(
    data: Data<AppState>,
    merchant_id: MerchantId,
    city: CityName,
    vehicle_type: VehicleType,
    ride_id: RideId,
    driver_id: DriverId,
    _driver_mode: Option<DriverMode>,
    _ride_status: RideStatus,
    locations: Vec<UpdateDriverLocationRequest>,
) -> () {
    if locations.len() > 100 {
        error!(
            "Way points more than 100 points {} on_ride: True",
            locations.len()
        );
    }
    let mut geo_entries = Vec::new();

    for loc in locations {
        geo_entries.push(Point {
            lat: loc.pt.lat,
            lon: loc.pt.lon,
        });

        let dimensions = Dimensions {
            merchant_id: merchant_id.clone(),
            city: city.clone(),
            vehicle_type: vehicle_type.clone(),
        };

        if data.include_on_ride_driver_for_nearby {
            let loc = loc.clone();
            let _ = &data
                .sender
                .send((dimensions, loc.pt.lat, loc.pt.lon, driver_id.clone()))
                .await;
        }

        // let _ = kafka_stream_updates(
        //     data.clone(),
        //     &merchant_id,
        //     &ride_id,
        //     loc,
        //     Some(ride_status.clone()),
        //     driver_mode.clone(),
        // )
        // .await;
    }

    let _ =
        push_on_ride_driver_location(data.clone(), &driver_id, &merchant_id, &city, &geo_entries)
            .await;

    let on_ride_driver_location_count =
        get_on_ride_driver_location_count(data.clone(), &driver_id, &merchant_id, &city).await;

    if let Ok(on_ride_driver_location_count) = on_ride_driver_location_count {
        if on_ride_driver_location_count >= data.batch_size {
            let on_ride_driver_locations = get_on_ride_driver_locations(
                data.clone(),
                &driver_id,
                &merchant_id,
                &city,
                on_ride_driver_location_count,
            )
            .await;

            if let Ok(on_ride_driver_locations) = on_ride_driver_locations {
                let _ = call_api::<APISuccess, BulkDataReq>(
                    Method::POST,
                    &data.bulk_location_callback_url,
                    vec![("content-type", "application/json")],
                    Some(BulkDataReq {
                        ride_id: ride_id.clone(),
                        driver_id: driver_id.clone(),
                        loc: on_ride_driver_locations,
                    }),
                )
                .await;
            }
        }
    }
}

async fn process_off_ride_driver_location(
    data: Data<AppState>,
    merchant_id: MerchantId,
    city: CityName,
    vehicle_type: VehicleType,
    driver_id: DriverId,
    _driver_mode: Option<DriverMode>,
    _ride_status: Option<RideStatus>,
    locations: Vec<UpdateDriverLocationRequest>,
) -> () {
    if locations.len() > 100 {
        error!(
            "Way points more than 100 points {} on_ride: False",
            locations.len()
        );
    }

    for loc in locations {
        let dimensions = Dimensions {
            merchant_id: merchant_id.clone(),
            city: city.clone(),
            vehicle_type: vehicle_type.clone(),
        };

        let loc = loc.clone();

        let _ = &data
            .sender
            .send((dimensions, loc.pt.lat, loc.pt.lon, driver_id.clone()))
            .await;

        // let _ = kafka_stream_updates(
        //     data.clone(),
        //     &merchant_id,
        //     &"".to_string(),
        //     loc,
        //     ride_status.clone(),
        //     driver_mode.clone(),
        // )
        // .await;
    }
}

pub async fn track_driver_location(
    data: Data<AppState>,
    ride_id: RideId,
) -> Result<DriverLocationResponse, AppError> {
    let driver_details = get_driver_details(data.clone(), &ride_id).await?;

    let current_ride_status = get_driver_ride_status(
        data.clone(),
        &driver_details.driver_id,
        &driver_details.merchant_id,
        &driver_details.city,
    )
    .await?;

    let current_ride_status = if current_ride_status == Some(RideStatus::NEW) {
        DriverRideStatus::PreRide
    } else if current_ride_status == Some(RideStatus::INPROGRESS) {
        DriverRideStatus::ActualRide
    } else {
        return Err(AppError::InvalidRequest(
            "Invalid ride status for tracking driver location".to_string(),
        ));
    };

    let driver_last_known_location_details =
        get_driver_location(data, &driver_details.driver_id).await?;

    Ok(DriverLocationResponse {
        curr_point: driver_last_known_location_details.location,
        total_distance: 0.0, // Backward Compatibility : To be removed
        status: current_ride_status,
        last_update: driver_last_known_location_details.timestamp,
    })
}
