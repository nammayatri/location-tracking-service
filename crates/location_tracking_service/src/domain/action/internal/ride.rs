use crate::redis::commands::*;
use crate::redis::keys::*;
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

    let value = RideDetails {
        ride_status: RideStatus::INPROGRESS,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let _ = data
        .generic_redis
        .set_with_expiry(
            &on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id),
            value,
            data.on_ride_expiry,
        )
        .await;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    let value = RideDetails {
        ride_status: RideStatus::COMPLETED,
        ride_id: ride_id.clone(),
    };
    let value = serde_json::to_string(&value).unwrap();

    let _ = data
        .generic_redis
        .set_with_expiry(
            &on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id),
            value,
            data.on_ride_expiry,
        )
        .await
        .unwrap();

    let on_ride_driver_locations = get_on_ride_driver_locations(
        data.clone(),
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

    let value = RideDetails {
        ride_status: request_body.ride_status,
        ride_id: request_body.ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let result = data
        .generic_redis
        .set_with_expiry(
            &on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id),
            value,
            data.on_ride_expiry,
        )
        .await;
    if result.is_err() {
        return Err(AppError::InternalServerError);
    }

    Ok(APISuccess::default())
}
