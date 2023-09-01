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
    HttpRequest,
};

use crate::{
    common::types::*,
    domain::{action::ui::location, types::ui::location::*},
};

use shared::tools::error::AppError;

#[post("/ui/driver/location")]
pub async fn update_driver_location(
    data: Data<AppState>,
    param_obj: Json<Vec<UpdateDriverLocationRequest>>,
    req: HttpRequest,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();

    if request_body.is_empty() {
        return Err(AppError::InvalidRequest(
            "Vec<UpdateDriverLocationRequest> is empty".to_string(),
        ));
    }

    let token = req
        .headers()
        .get("token")
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|dm_str| Some(dm_str.to_string()))
        .ok_or(AppError::InvalidRequest("Token not found".to_string()))?;

    let merchant_id = req
        .headers()
        .get("mId")
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|dm_str| Some(dm_str.to_string()))
        .ok_or(AppError::InvalidRequest("mId not found".to_string()))?;

    let vehicle_type = req
        .headers()
        .get("vt")
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|dm_str| VehicleType::from_str(dm_str).ok())
        .ok_or(AppError::InvalidRequest(
            "VehicleType not found".to_string(),
        ))?;

    let driver_mode = req
        .headers()
        .get("dm")
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|dm_str| DriverMode::from_str(dm_str).ok());

    Ok(Json(
        location::update_driver_location(
            token,
            merchant_id,
            vehicle_type,
            data,
            request_body,
            driver_mode,
        )
        .await?,
    ))
}

#[get("/ui/driver/location/{rideId}")]
async fn track_driver_location(
    data: Data<AppState>,
    path: Path<String>,
) -> Result<Json<DriverLocationResponse>, AppError> {
    let ride_id = path.into_inner();

    Ok(Json(location::track_driver_location(data, ride_id).await?))
}
