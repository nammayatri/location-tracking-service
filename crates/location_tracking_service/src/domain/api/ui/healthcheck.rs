use actix_web::{
    get,
    web::{Data, Json},
};

use crate::{
    common::types::*, domain::types::internal::ride::ResponseData, redis::keys::health_check_key,
};

use shared::tools::error::AppError;

#[get("/healthcheck")]
async fn health_check(data: Data<AppState>) -> Result<Json<ResponseData>, AppError> {
    let _ = data
        .generic_redis
        .set_key(&health_check_key(), "driver-location-service-health-check")
        .await;

    let health_check_resp = data.generic_redis.get_key(&health_check_key()).await?;

    if health_check_resp.is_none() {
        return Err(AppError::InternalServerError);
    }

    Ok(Json(ResponseData {
        result: "Service Is Up".to_string(),
    }))
}
