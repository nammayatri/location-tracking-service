/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::redis::commands::*;

use crate::{
    common::{types::*, utils::get_city},
    domain::types::internal::ride::*,
};
use actix_web::web::Data;
use shared::tools::error::AppError;
use tracing::info;

async fn update_driver_location(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    city: &CityName,
    lat: Latitude,
    lon: Longitude,
    driver_mode: Option<DriverMode>,
) -> Result<(), AppError> {
    let _ = set_driver_last_location_update(
        data.clone(),
        &driver_id,
        &merchant_id,
        &Point { lat, lon },
        driver_mode,
    )
    .await?;

    let _ = push_on_ride_driver_location(
        data,
        &driver_id,
        &merchant_id,
        &city,
        &vec![Point { lat, lon }],
    )
    .await?;

    Ok(())
}

pub async fn ride_start(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> Result<APISuccess, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;
    let current_ride_status = get_driver_ride_status(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
    )
    .await?;
    if current_ride_status != Some(RideStatus::NEW) {
        return Err(AppError::InvalidRideStatus(ride_id));
    }
    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &city,
        &request_body.driver_id,
        ride_id.clone(),
        RideStatus::INPROGRESS,
    )
    .await?;

    let _ = update_driver_location(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
        request_body.lat,
        request_body.lon,
        None,
    )
    .await?;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;
    let current_ride_status = get_driver_ride_status(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
    )
    .await?;
    if current_ride_status != Some(RideStatus::INPROGRESS) {
        return Err(AppError::InvalidRideStatus(ride_id));
    }
    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &city,
        &request_body.driver_id,
        ride_id.clone(),
        RideStatus::COMPLETED,
    )
    .await?;

    let _ = update_driver_location(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
        request_body.lat,
        request_body.lon,
        None,
    )
    .await?;

    let on_ride_driver_location_count = get_on_ride_driver_location_count(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
    )
    .await?;

    let on_ride_driver_locations = get_on_ride_driver_locations(
        data,
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
        on_ride_driver_location_count as usize,
    )
    .await?;

    Ok(RideEndResponse {
        ride_id: ride_id,
        driver_id: request_body.driver_id,
        loc: on_ride_driver_locations,
    })
}

pub async fn ride_details(
    data: Data<AppState>,
    request_body: RideDetailsRequest,
) -> Result<APISuccess, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;
    let current_ride_status = get_driver_ride_status(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
    )
    .await?;

    match request_body.ride_status {
        RideStatus::NEW => {
            if current_ride_status == Some(RideStatus::INPROGRESS) {
                return Err(AppError::InvalidRideStatus(request_body.ride_id));
            }
        }
        _ => return Err(AppError::InvalidRideStatus(request_body.ride_id)),
    }

    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &city,
        &request_body.driver_id,
        request_body.ride_id.clone(),
        request_body.ride_status,
    )
    .await?;

    let _ = update_driver_location(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
        request_body.lat,
        request_body.lon,
        None,
    )
    .await?;

    let driver_details = DriverDetails {
        driver_id: request_body.driver_id,
        merchant_id: request_body.merchant_id,
        city,
    };

    set_driver_details(data.clone(), &request_body.ride_id, driver_details).await?;

    Ok(APISuccess::default())
}
