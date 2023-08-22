use actix_web::web::Data;

use crate::{
    common::{redis::*, types::*},
    domain::types::internal::driver::*,
};
use shared::tools::error::AppError;

pub async fn driver_details(
    data: Data<AppState>,
    request_body: DriverDetailsRequest,
) -> Result<APISuccess, AppError> {
    let key = driver_details_key(&request_body.driver_id);
    let value = DriverDetails {
        driver_id: request_body.driver_id,
        driver_mode: request_body.driver_mode,
    };
    let value = serde_json::to_string(&value).unwrap();

    let redis_pool = data.generic_redis.lock().await;
    let result = redis_pool
        .set_with_expiry(&key, value, data.on_ride_expiry)
        .await;
    if result.is_err() {
        return Err(AppError::InternalServerError);
    }

    Ok(APISuccess::default())
}
