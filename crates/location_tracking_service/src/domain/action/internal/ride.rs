/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::environment::AppState;
use crate::redis::commands::*;

use crate::{
    common::{types::*, utils::get_city},
    domain::types::internal::ride::*,
};
use actix_web::web::Data;
use shared::tools::error::AppError;

async fn update_driver_location(
    data: Data<AppState>,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    lat: Latitude,
    lon: Longitude,
    driver_mode: Option<DriverMode>,
) -> Result<(), AppError> {
    set_driver_last_location_update(
        data.clone(),
        driver_id,
        merchant_id,
        &Point { lat, lon },
        driver_mode,
    )
    .await?;

    push_on_ride_driver_locations(data, driver_id, merchant_id, &vec![Point { lat, lon }]).await?;

    Ok(())
}

pub async fn ride_start(
    RideId(ride_id): RideId,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> Result<APISuccess, AppError> {
    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &request_body.driver_id,
        RideId(ride_id),
        RideStatus::INPROGRESS,
    )
    .await?;

    update_driver_location(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        request_body.lat,
        request_body.lon,
        None,
    )
    .await?;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    RideId(ride_id): RideId,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &request_body.driver_id,
        RideId(ride_id.clone()),
        RideStatus::COMPLETED,
    )
    .await?;

    update_driver_location(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
        request_body.lat,
        request_body.lon,
        None,
    )
    .await?;

    let on_ride_driver_location_count = get_on_ride_driver_location_count(
        data.clone(),
        &request_body.driver_id,
        &request_body.merchant_id,
    )
    .await?;

    let on_ride_driver_locations = get_on_ride_driver_locations(
        data,
        &request_body.driver_id,
        &request_body.merchant_id,
        on_ride_driver_location_count,
    )
    .await?;

    Ok(RideEndResponse {
        ride_id: RideId(ride_id),
        driver_id: request_body.driver_id,
        loc: on_ride_driver_locations,
    })
}

pub async fn ride_details(
    data: Data<AppState>,
    request_body: RideDetailsRequest,
) -> Result<APISuccess, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &request_body.driver_id,
        request_body.ride_id.clone(),
        request_body.ride_status,
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
