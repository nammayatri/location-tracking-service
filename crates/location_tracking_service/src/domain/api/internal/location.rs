use actix_web::{HttpRequest, web::{Data, Json}, get};

use crate::{common::{types::*, errors::AppError}, domain::{types::internal::location::*, action::internal::*}};

#[get("/internal/drivers/nearby")]
async fn get_nearby_drivers(
    data: Data<AppState>,
    param_obj: Json<NearbyDriversRequest>,
    _req: HttpRequest,
) -> Result<Json<NearbyDriverResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(location::get_nearby_drivers(data, request_body).await?))
}