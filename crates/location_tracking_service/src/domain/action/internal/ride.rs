use crate::redis::commands::*;

use crate::{
    common::{types::*, utils::get_city},
    domain::types::internal::ride::*,
};
use actix_web::web::Data;
use shared::tools::error::AppError;

pub async fn ride_start(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> Result<APISuccess, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;
    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &city,
        &request_body.driver_id,
        ride_id.clone(),
        RideStatus::INPROGRESS,
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
    set_ride_details(
        data.clone(),
        &request_body.merchant_id,
        &city,
        &request_body.driver_id,
        ride_id.clone(),
        RideStatus::COMPLETED,
    )
    .await?;

    let on_ride_driver_locations = get_on_ride_driver_locations(
        data,
        &request_body.driver_id,
        &request_body.merchant_id,
        &city,
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
    set_ride_details(
        data,
        &request_body.merchant_id,
        &city,
        &request_body.driver_id,
        request_body.ride_id,
        request_body.ride_status,
    )
    .await?;

    Ok(APISuccess::default())
}
