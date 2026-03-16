/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{
    get, post,
    web::{Data, Json, Path},
    HttpRequest,
};

use crate::tools::error::AppError;
use crate::{
    common::types::*,
    domain::{action::internal::*, types::internal::location::*},
    environment::AppState,
};

#[get("/internal/drivers/nearby")]
async fn get_nearby_drivers(
    data: Data<AppState>,
    param_obj: Json<NearbyDriversRequest>,
    _req: HttpRequest,
) -> Result<Json<NearbyDriverResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(
        location::get_nearby_drivers(data, request_body).await?,
    ))
}

#[get("/internal/drivers/location")]
async fn get_drivers_location(
    data: Data<AppState>,
    param_obj: Json<GetDriversLocationRequest>,
    _req: HttpRequest,
) -> Result<Json<GetDriversLocationResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(
        location::get_drivers_location(data, request_body.driver_ids).await?,
    ))
}

#[post("/internal/driver/blockTill")]
async fn driver_block_till(
    data: Data<AppState>,
    param_obj: Json<DriverBlockTillRequest>,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(location::driver_block_till(data, request_body).await?))
}

#[get("/internal/trackVehicles")]
async fn track_vehicles(
    data: Data<AppState>,
    param_obj: Json<TrackVehicleRequest>,
) -> Result<Json<TrackVehiclesResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(location::track_vehicles(data, request_body).await?))
}

#[post("/internal/trackVehicles")]
async fn post_track_vehicles(
    data: Data<AppState>,
    param_obj: Json<TrackVehicleRequest>,
) -> Result<Json<TrackVehiclesResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(location::track_vehicles(data, request_body).await?))
}

#[get("/internal/special-locations/{special_location_id}/drivers")]
async fn get_special_location_drivers(
    data: Data<AppState>,
    path: Path<(String,)>,
) -> Result<Json<SpecialLocationDriversResponse>, AppError> {
    let special_location_id = path.into_inner().0;
    Ok(Json(
        location::get_special_location_drivers(data, special_location_id).await?,
    ))
}

#[get("/internal/special-locations/{special_location_id}/queue/{vehicle_type}/drivers/{driver_id}/position")]
async fn get_driver_queue_position(
    data: Data<AppState>,
    path: Path<(String, String, String)>,
) -> Result<Json<DriverQueuePositionResponse>, AppError> {
    let (special_location_id, vehicle_type, driver_id) = path.into_inner();
    Ok(Json(
        location::driver_queue_position(data, special_location_id, vehicle_type, driver_id).await?,
    ))
}
