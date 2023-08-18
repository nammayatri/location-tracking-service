use actix_web::{HttpRequest, web::{Data, Json}, post, HttpResponse};

use crate::{domain::{action::ui::location, types::ui::location::UpdateDriverLocationRequest}, common::types::*};

#[post("/ui/driver/location")]
pub async fn update_driver_location(
    data: Data<AppState>,
    param_obj: Json<Vec<UpdateDriverLocationRequest>>,
    req: HttpRequest,
) -> HttpResponse {
    let request_body = param_obj.into_inner();

    let token: Token = req
        .headers()
        .get("token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let merchant_id: MerchantId = req
        .headers()
        .get("mId")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let vehicle_type: VehicleType = req
        .headers()
        .get("vt")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    location::update_driver_location(token, merchant_id, vehicle_type, data, request_body).await
}