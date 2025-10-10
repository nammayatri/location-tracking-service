/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use std::str::FromStr;

use actix_web::{
    get, post,
    web::{Data, Json, Path},
    HttpRequest, HttpResponse,
};

use crate::{
    common::types::*,
    domain::{action::ui::location, types::ui::location::*},
    environment::AppState,
};

use crate::tools::error::AppError;

#[post("/ui/driver/location")]
pub async fn update_driver_location(
    data: Data<AppState>,
    param_obj: Json<Vec<UpdateDriverLocationRequest>>,
    req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    let request_body = param_obj.into_inner();

    if request_body.is_empty() {
        return Ok(HttpResponse::Ok().finish());
    }

    let token = req
        .headers()
        .get("token")
        .and_then(|header_value| header_value.to_str().ok())
        .map(|dm_str| dm_str.to_string())
        .ok_or(AppError::InvalidRequest(
            "token (Header) not found".to_string(),
        ))?;

    let vehicle_type = req
        .headers()
        .get("vt")
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|dm_str| VehicleType::from_str(dm_str).ok())
        .ok_or(AppError::InvalidRequest(
            "vt (VehicleType - Header) not found".to_string(),
        ))?;

    let driver_mode = req
        .headers()
        .get("dm")
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|dm_str| DriverMode::from_str(dm_str).ok())
        .ok_or(AppError::InvalidRequest(
            "dm (DriverMode - Header) not found".to_string(),
        ))?;

    let group_id = req
        .headers()
        .get("gid")
        .and_then(|header_value| header_value.to_str().ok())
        .map(|gid_str| gid_str.to_string());

    let group_id2 = req
        .headers()
        .get("gid2")
        .and_then(|header_value| header_value.to_str().ok())
        .map(|gid_str| gid_str.to_string());

    let req_merchant_id = req
        .headers()
        .get("mid")
        .and_then(|header_value| header_value.to_str().ok())
        .map(|mid_str| MerchantId(mid_str.to_string()));

    location::update_driver_location_by_token(
        Token(token),
        vehicle_type,
        data,
        request_body,
        driver_mode,
        group_id,
        group_id2,
        req_merchant_id,
    )
    .await?;
    Ok(HttpResponse::Ok().finish())
}

#[get("/ui/driver/location/{rideId}")]
async fn track_driver_location(
    data: Data<AppState>,
    path: Path<String>,
) -> Result<Json<DriverLocationResponse>, AppError> {
    let ride_id = RideId(path.into_inner());

    Ok(Json(location::track_driver_location(data, ride_id).await?))
}
