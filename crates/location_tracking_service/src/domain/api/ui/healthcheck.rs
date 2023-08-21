
use actix_web::{
    get,
    web::{Data,Json}
};

use crate::{
    common::{types::*, errors::AppError},
    domain::types::internal::ride::ResponseData,
};

#[get("/healthcheck")]
async fn health_check(data: Data<AppState>) -> Result<Json<ResponseData>, AppError> {
    let redis_pool = data.generic_redis.lock().await;

    let health_check_key = format!("health_check");
    _ = redis_pool
        .set_key(&health_check_key, "driver-location-service-health-check")
        .await;
    let health_check_resp = redis_pool
        .get_key::<String>(&health_check_key)
        .await
        .unwrap();
    let nil_string = String::from("nil");

    if health_check_resp == nil_string {
        return Err(AppError::InternalServerError);
    }

   Ok(Json(ResponseData{result :"Service Is Up".to_string()}))
}