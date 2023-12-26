/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::environment::AppState;
use crate::redis::commands::*;
use crate::tools::error::AppError;
use crate::{common::types::*, domain::types::internal::ride::*};
use actix_web::web::Data;

pub async fn ride_create(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideRequest,
) -> Result<APISuccess, AppError> {
    set_ride_details(
        &data.persistent_redis,
        &data.redis_expiry,
        &request_body.merchant_id,
        &request_body.driver_id,
        ride_id.to_owned(),
        RideStatus::NEW,
    )
    .await?;

    let driver_details = DriverDetails {
        driver_id: request_body.driver_id,
    };

    set_driver_details(
        &data.persistent_redis,
        &data.redis_expiry,
        &ride_id,
        driver_details,
    )
    .await?;

    Ok(APISuccess::default())
}

pub async fn ride_start(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideRequest,
) -> Result<APISuccess, AppError> {
    set_ride_details(
        &data.persistent_redis,
        &data.redis_expiry,
        &request_body.merchant_id,
        &request_body.driver_id,
        ride_id,
        RideStatus::INPROGRESS,
    )
    .await?;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    let mut on_ride_driver_locations = get_on_ride_driver_locations(
        &data.persistent_redis,
        &request_body.driver_id,
        &request_body.merchant_id,
        data.batch_size,
    )
    .await?;

    on_ride_driver_locations.push(Point {
        lat: request_body.lat,
        lon: request_body.lon,
    });

    ride_cleanup(
        &data.persistent_redis,
        &request_body.merchant_id,
        &request_body.driver_id,
        &ride_id,
    )
    .await?;

    Ok(RideEndResponse {
        ride_id,
        driver_id: request_body.driver_id,
        loc: on_ride_driver_locations,
    })
}

pub async fn ride_clear(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideRequest,
) -> Result<APISuccess, AppError> {
    ride_cleanup(
        &data.persistent_redis,
        &request_body.merchant_id,
        &request_body.driver_id,
        &ride_id,
    )
    .await?;

    Ok(APISuccess::default())
}

// TODO :: To be deprecated...
pub async fn ride_details(
    data: Data<AppState>,
    request_body: RideDetailsRequest,
) -> Result<APISuccess, AppError> {
    if let RideStatus::CANCELLED = request_body.ride_status {
        ride_cleanup(
            &data.persistent_redis,
            &request_body.merchant_id,
            &request_body.driver_id,
            &request_body.ride_id,
        )
        .await?;
    } else {
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
        };

        set_driver_details(
            &data.persistent_redis,
            &data.redis_expiry,
            &request_body.ride_id,
            driver_details,
        )
        .await?;
    }

    Ok(APISuccess::default())
}
