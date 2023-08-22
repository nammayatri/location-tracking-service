use actix_web::{
    post,
    web::{Data, Json},
};

use crate::{
    common::types::*,
    domain::{action::internal::*, types::internal::driver::*},
};
use shared::tools::error::AppError;

#[post("/internal/driver/driverDetails")]
async fn driver_details(
    data: Data<AppState>,
    param_obj: Json<DriverDetailsRequest>,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(driver::driver_details(data, request_body).await?))
}
