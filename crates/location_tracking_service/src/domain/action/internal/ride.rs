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
use chrono::Utc;
use shared::redis::types::RedisConnectionPool;
use shared::tools::error::AppError;
use std::sync::Arc;

#[allow(clippy::too_many_arguments)]
async fn update_driver_location(
    persistent_redis: &Arc<RedisConnectionPool>,
    last_location_timstamp_expiry: &u32,
    redis_expiry: &u32,
    driver_id: &DriverId,
    merchant_id: &MerchantId,
    lat: Latitude,
    lon: Longitude,
) -> Result<i64, AppError> {
    set_driver_last_location_update(
        persistent_redis,
        last_location_timstamp_expiry,
        driver_id,
        merchant_id,
        &Point { lat, lon },
        &TimeStamp(Utc::now()),
    )
    .await?;

    push_on_ride_driver_locations(
        persistent_redis,
        driver_id,
        merchant_id,
        &[Point { lat, lon }],
        redis_expiry,
    )
    .await
}

pub async fn ride_start(
    RideId(ride_id): RideId,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> Result<APISuccess, AppError> {
    set_ride_details(
        &data.persistent_redis,
        &data.redis_expiry,
        &request_body.merchant_id,
        &request_body.driver_id,
        RideId(ride_id),
        RideStatus::INPROGRESS,
    )
    .await?;

    update_driver_location(
        &data.persistent_redis,
        &data.last_location_timstamp_expiry,
        &data.redis_expiry,
        &request_body.driver_id,
        &request_body.merchant_id,
        request_body.lat,
        request_body.lon,
    )
    .await?;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    clear_ride_details(
        &data.persistent_redis,
        &request_body.merchant_id,
        &request_body.driver_id,
        &ride_id,
    )
    .await?;

    let on_ride_driver_location_count = update_driver_location(
        &data.persistent_redis,
        &data.last_location_timstamp_expiry,
        &data.redis_expiry,
        &request_body.driver_id,
        &request_body.merchant_id,
        request_body.lat,
        request_body.lon,
    )
    .await?;

    let on_ride_driver_locations = get_on_ride_driver_locations(
        &data.persistent_redis,
        &request_body.driver_id,
        &request_body.merchant_id,
        on_ride_driver_location_count,
    )
    .await?;

    Ok(RideEndResponse {
        ride_id,
        driver_id: request_body.driver_id,
        loc: on_ride_driver_locations,
    })
}

pub async fn ride_details(
    data: Data<AppState>,
    request_body: RideDetailsRequest,
) -> Result<APISuccess, AppError> {
    let city = get_city(&request_body.lat, &request_body.lon, &data.polygon)?;

    set_ride_details(
        &data.persistent_redis,
        &data.redis_expiry,
        &request_body.merchant_id,
        &request_body.driver_id,
        request_body.ride_id.to_owned(),
        request_body.ride_status,
    )
    .await?;

    let driver_details = DriverDetails {
        driver_id: request_body.driver_id,
        merchant_id: request_body.merchant_id,
        city: Some(city),
    };

    set_driver_details(
        &data.persistent_redis,
        &data.redis_expiry,
        &request_body.ride_id,
        driver_details,
    )
    .await?;

    Ok(APISuccess::default())
}
