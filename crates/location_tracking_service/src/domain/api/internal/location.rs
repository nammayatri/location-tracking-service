use actix_web::{
    get,
    web::{Data, Json},
    HttpRequest,
};

use crate::{
    common::{errors::AppError, types::*},
    domain::{action::internal::*, types::internal::location::*},
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
