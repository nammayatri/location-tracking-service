/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{
    get,
    web::{Data, Json},
};

use crate::{
    domain::types::internal::ride::ResponseData, environment::AppState,
    redis::keys::health_check_key,
};

use crate::tools::error::AppError;

#[get("/healthcheck")]
async fn health_check(data: Data<AppState>) -> Result<Json<ResponseData>, AppError> {
    data.redis
        .set_key_as_str(
            &health_check_key(),
            "driver-location-service-health-check",
            data.redis_expiry,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    let health_check_resp = data
        .redis
        .get_key_as_str(&health_check_key())
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    if health_check_resp.is_none() {
        return Err(AppError::InternalError(
            "Health check failed as cannot get key from redis".to_string(),
        ));
    }

    Ok(Json(ResponseData {
        result: "Service Is Up".to_string(),
    }))
}
