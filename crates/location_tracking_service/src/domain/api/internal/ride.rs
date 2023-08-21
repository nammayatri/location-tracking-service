use actix_web::{web::{Data, Json, Path}, post};

use crate::{common::{types::*, errors::AppError}, domain::{types::internal::ride::*, action::internal::*}};

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