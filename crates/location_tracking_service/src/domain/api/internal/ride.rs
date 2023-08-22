use actix_web::{
    post,
    web::{Data, Json, Path},
};

use crate::{
    common::{errors::AppError, types::*},
    domain::{action::internal::*, types::internal::ride::*},
};

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
