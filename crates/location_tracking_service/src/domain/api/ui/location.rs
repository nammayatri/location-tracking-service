use std::str::FromStr;

use actix_web::{
    post,
    web::{Data, Json},
    HttpRequest,
};

use crate::{
    common::types::*,
    domain::{action::ui::location, types::ui::location::UpdateDriverLocationRequest},
};

use shared::tools::error::AppError;

#[post("/ui/driver/location")]
pub async fn update_driver_location(
    data: Data<AppState>,
    param_obj: Json<Vec<UpdateDriverLocationRequest>>,
    req: HttpRequest,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();

    let token: Token = req
        .headers()
        .get("token")
        .expect("token not found in headers")
        .to_str()
        .map_err(|err| AppError::InternalError(err.to_string()))?
        .to_string();

    let merchant_id: MerchantId = req
        .headers()
        .get("mId")
        .expect("mId not found in headers")
        .to_str()
        .map_err(|err| AppError::InternalError(err.to_string()))?
        .to_string();

    let vehicle_type: VehicleType = VehicleType::from_str(
        req.headers()
            .get("vt")
            .expect("vt not found in headers")
            .to_str()
            .map_err(|err| AppError::InternalError(err.to_string()))?,
    )
    .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(
        location::update_driver_location(token, merchant_id, vehicle_type, data, request_body)
            .await?,
    ))
}
