use actix_web::{
    get,
    web::{Data, Json},
};

use crate::{
    common::{errors::AppError, types::*, redis::*},
    domain::types::internal::ride::ResponseData,
};

#[get("/healthcheck")]
async fn health_check(data: Data<AppState>) -> Result<Json<ResponseData>, AppError> {
    let _ = data
        .generic_redis
        .lock()
        .await
        .set_key(&health_check_key(), "driver-location-service-health-check")
        .await;

    let health_check_resp = data
        .generic_redis
        .lock()
        .await
        .get_key::<String>(&health_check_key())
        .await
        .unwrap();

    if health_check_resp == String::from("nil") {
        return Err(AppError::InternalServerError);
    }

    Ok(Json(ResponseData {
        result: "Service Is Up".to_string(),
    }))
}
