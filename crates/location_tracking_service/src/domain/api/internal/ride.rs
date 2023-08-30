/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{
    post,
    web::{Data, Json, Path},
};

use crate::{
    common::types::*,
    domain::{action::internal::*, types::internal::ride::*},
};
use shared::tools::error::AppError;

#[post("/internal/ride/{rideId}/start")]
async fn ride_start(
    data: Data<AppState>,
    param_obj: Json<RideStartRequest>,
    path: Path<String>,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();
    let ride_id = path.into_inner();

    Ok(Json(ride::ride_start(ride_id, data, request_body).await?))
}

#[post("/internal/ride/{rideId}/end")]
async fn ride_end(
    data: Data<AppState>,
    param_obj: Json<RideEndRequest>,
    path: Path<String>,
) -> Result<Json<RideEndResponse>, AppError> {
    let request_body = param_obj.into_inner();
    let ride_id = path.into_inner();

    Ok(Json(ride::ride_end(ride_id, data, request_body).await?))
}

#[post("/internal/ride/rideDetails")]
async fn ride_details(
    data: Data<AppState>,
    param_obj: Json<RideDetailsRequest>,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(ride::ride_details(data, request_body).await?))
}
