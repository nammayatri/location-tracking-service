use actix_web::{HttpRequest, web::{Data, Json}, get, HttpResponse};

use crate::{common::types::*, domain::{types::internal::location::*, action::internal::*}};

#[get("/internal/drivers/nearby")]
async fn get_nearby_drivers(
    data: Data<AppState>,
    param_obj: Json<NearbyDriversRequest>,
    _req: HttpRequest,
) -> HttpResponse {
    let request_body = param_obj.into_inner();

    location::get_nearby_drivers(data, request_body).await
}