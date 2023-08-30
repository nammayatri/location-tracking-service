use actix_web::{
    get,
    web::{Data, Json},
    HttpRequest,
};

use crate::{
    common::types::*,
    domain::{action::internal::*, types::internal::location::*},
};
use shared::tools::error::AppError;

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
