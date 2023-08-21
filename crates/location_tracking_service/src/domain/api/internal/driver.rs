use actix_web::{web::{Data, Json}, post};

use crate::{common::{types::*, errors::AppError}, domain::{types::internal::driver::*, action::internal::*}};

#[post("/internal/driver/driverDetails")]
async fn driver_details(
    data: Data<AppState>,
    param_obj: Json<DriverDetailsRequest>,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(driver::driver_details(data, request_body).await?))
}
