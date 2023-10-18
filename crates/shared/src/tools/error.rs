/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use actix_web::{
    http::{header::ContentType, StatusCode},
    HttpResponse, ResponseError,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    error_message: String,
    pub error_code: String,
}

#[macros::add_error]
pub enum AppError {
    InternalError(String),
    InvalidRequest(String),
    DriverRideDetailsNotFound,
    DriverLastKnownLocationNotFound,
    DriverLastLocationTimestampNotFound,
    UnprocessibleRequest(String),
    InvalidRideStatus(String, String),
    ExternalAPICallError(String),
    SerializationError(String),
    DeserializationError(String),
    DriverAppAuthFailed(String, String),
    Unserviceable(f64, f64),
    HitsLimitExceeded(String),
    DriverBulkLocationUpdateFailed(String),
    InvalidConfiguration(String),
    SetFailed,
    SetExFailed,
    SetExpiryFailed,
    GetFailed,
    MGetFailed,
    DeleteFailed,
    SetHashFieldFailed,
    GetHashFieldFailed,
    RPushFailed,
    RPopFailed,
    LPopFailed,
    LRangeFailed,
    LLenFailed,
    NotFound,
    InvalidRedisEntryId,
    RedisConnectionError,
    SubscribeError,
    PublishError,
    GeoAddFailed,
    ZAddFailed,
    ZremrangeByRankFailed,
    GeoSearchFailed,
    ZCardFailed,
    GeoPosFailed,
    ZRangeFailed,
}

impl AppError {
    fn error_message(&self) -> ErrorBody {
        ErrorBody {
            error_message: self.message(),
            error_code: self.code(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            AppError::InternalError(err) => err.to_string(),
            AppError::InvalidRequest(err) => err.to_string(),
            AppError::UnprocessibleRequest(err) => err.to_string(),
            AppError::InvalidRideStatus(ride_id, ride_status) => {
                format!("Invalid Ride Status : RideId - {ride_id}, Ride Status - {ride_status}")
            }
            AppError::ExternalAPICallError(err) => err.to_string(),
            AppError::SerializationError(err) => err.to_string(),
            AppError::DeserializationError(err) => err.to_string(),
            AppError::HitsLimitExceeded(err) => err.to_string(),
            AppError::DriverAppAuthFailed(token, err) => {
                format!("Invalid Token - {token} : {err}")
            }
            AppError::DriverBulkLocationUpdateFailed(err) => {
                format!("Driver Bulk Location Update Failed : {err}")
            }
            AppError::Unserviceable(lat, lon) => {
                format!("Location is unserviceable : (Lat : {lat}, Lon : {lon})")
            }
            _ => "".to_string(),
        }
    }

    fn code(&self) -> String {
        match self {
            AppError::InternalError(_) => "INTERNAL_ERROR",
            AppError::InvalidRequest(_) => "INVALID_REQUEST",
            AppError::DriverRideDetailsNotFound => "DRIVER_RIDE_DETAILS_NOT_FOUND",
            AppError::DriverLastKnownLocationNotFound => "DRIVER_LAST_KNOWN_LOCATION_NOT_FOUND",
            AppError::DriverLastLocationTimestampNotFound => {
                "DRIVER_LAST_LOCATION_TIMESTAMP_NOT_FOUND"
            }
            AppError::UnprocessibleRequest(_) => "UNPROCESSIBLE_REQUEST",
            AppError::InvalidRideStatus(_, _) => "INVALID_RIDE_STATUS",
            AppError::ExternalAPICallError(_) => "EXTERNAL_API_CALL_ERROR",
            AppError::SerializationError(_) => "SERIALIZATION_ERROR",
            AppError::DeserializationError(_) => "DESERIALIZATION_ERROR",
            AppError::DriverAppAuthFailed(_, _) => "INVALID_TOKEN",
            AppError::Unserviceable(_, _) => "LOCATION_NOT_SERVICEABLE",
            AppError::HitsLimitExceeded(_) => "HITS_LIMIT_EXCEED",
            AppError::DriverBulkLocationUpdateFailed(_) => "DOBPP_BULK_LOCATION_UPDATE_FAILED",
            AppError::InvalidConfiguration(_) => "INVALID_REDIS_CONFIGURATION",
            AppError::SetFailed => "SET_FAILED",
            AppError::SetExFailed => "SET_EX_FAILED",
            AppError::SetExpiryFailed => "SET_EXPIRY_FAILED",
            AppError::GetFailed => "GET_FAILED",
            AppError::MGetFailed => "MGET_FAILED",
            AppError::DeleteFailed => "DELETE_FAILED",
            AppError::SetHashFieldFailed => "SETHASHFIELD_FAILED",
            AppError::GetHashFieldFailed => "GETHASHFIELD_FAILED",
            AppError::RPushFailed => "RPUSH_FAILED",
            AppError::RPopFailed => "RPOP_FAILED",
            AppError::LPopFailed => "LPOP_FAILED",
            AppError::LRangeFailed => "LRANGE_FAILED",
            AppError::LLenFailed => "LLEN_FAILED",
            AppError::NotFound => "NOT_FOUND",
            AppError::InvalidRedisEntryId => "INVALID_REDIS_ENTRY_ID",
            AppError::RedisConnectionError => "REDIS_CONNECTION_FAILED",
            AppError::SubscribeError => "SUBSCRIBE_FAILED",
            AppError::PublishError => "PUBLISH_FAILED",
            AppError::GeoAddFailed => "GEOADD_FAILED",
            AppError::ZAddFailed => "ZADD_FAILED",
            AppError::ZremrangeByRankFailed => "ZREMRANGEBYRANK_FAILED",
            AppError::GeoSearchFailed => "GEOSEARCH_FAILED",
            AppError::ZCardFailed => "ZCARD_FAILED",
            AppError::GeoPosFailed => "GEOPOS_FAILED",
            AppError::ZRangeFailed => "ZRANGE_FAILED",
        }
        .to_string()
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::json())
            .json(self.error_message())
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            AppError::DriverRideDetailsNotFound => StatusCode::BAD_REQUEST,
            AppError::DriverLastKnownLocationNotFound => StatusCode::BAD_REQUEST,
            AppError::DriverLastLocationTimestampNotFound => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::UnprocessibleRequest(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::InvalidRideStatus(_, _) => StatusCode::BAD_REQUEST,
            AppError::ExternalAPICallError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DeserializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DriverAppAuthFailed(_, _) => StatusCode::UNAUTHORIZED,
            AppError::Unserviceable(_, _) => StatusCode::BAD_REQUEST,
            AppError::HitsLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::DriverBulkLocationUpdateFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InvalidConfiguration(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetExFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetExpiryFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GetFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::MGetFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DeleteFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetHashFieldFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GetHashFieldFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::InvalidRedisEntryId => StatusCode::BAD_REQUEST,
            AppError::RedisConnectionError => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SubscribeError => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::PublishError => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GeoAddFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZAddFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZremrangeByRankFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GeoSearchFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZCardFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GeoPosFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZRangeFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::RPushFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::RPopFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::LPopFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::LRangeFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::LLenFailed => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
