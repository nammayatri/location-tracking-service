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
    // API Errors
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

    // Redis Errors
    RedisConnectionError,
    SetFailed(String),
    SetExFailed(String),
    SetExpiryFailed(String),
    GetFailed(String),
    MGetFailed(String),
    DeleteFailed(String),
    SetHashFieldFailed(String),
    GetHashFieldFailed(String),
    RPushFailed(String),
    RPopFailed(String),
    LPopFailed(String),
    LRangeFailed(String),
    LLenFailed(String),
    NotFound(String),
    InvalidRedisEntryId(String),
    SubscribeError(String),
    PublishError(String),
    GeoAddFailed(String),
    ZAddFailed(String),
    ZremrangeByRankFailed(String),
    GeoSearchFailed(String),
    ZCardFailed(String),
    GeoPosFailed(String),
    ZRangeFailed(String),
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
            AppError::SetFailed(err) => format!("Redis Error : {err}"),
            AppError::SetExFailed(err) => format!("Redis Error : {err}"),
            AppError::SetExpiryFailed(err) => format!("Redis Error : {err}"),
            AppError::GetFailed(err) => format!("Redis Error : {err}"),
            AppError::MGetFailed(err) => format!("Redis Error : {err}"),
            AppError::DeleteFailed(err) => format!("Redis Error : {err}"),
            AppError::SetHashFieldFailed(err) => format!("Redis Error : {err}"),
            AppError::GetHashFieldFailed(err) => format!("Redis Error : {err}"),
            AppError::RPushFailed(err) => format!("Redis Error : {err}"),
            AppError::RPopFailed(err) => format!("Redis Error : {err}"),
            AppError::LPopFailed(err) => format!("Redis Error : {err}"),
            AppError::LRangeFailed(err) => format!("Redis Error : {err}"),
            AppError::LLenFailed(err) => format!("Redis Error : {err}"),
            AppError::NotFound(err) => format!("Redis Error : {err}"),
            AppError::InvalidRedisEntryId(err) => format!("Redis Error : {err}"),
            AppError::SubscribeError(err) => format!("Redis Error : {err}"),
            AppError::PublishError(err) => format!("Redis Error : {err}"),
            AppError::GeoAddFailed(err) => format!("Redis Error : {err}"),
            AppError::ZAddFailed(err) => format!("Redis Error : {err}"),
            AppError::ZremrangeByRankFailed(err) => format!("Redis Error : {err}"),
            AppError::GeoSearchFailed(err) => format!("Redis Error : {err}"),
            AppError::ZCardFailed(err) => format!("Redis Error : {err}"),
            AppError::GeoPosFailed(err) => format!("Redis Error : {err}"),
            AppError::ZRangeFailed(err) => format!("Redis Error : {err}"),
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
            AppError::SetFailed(_) => "SET_FAILED",
            AppError::SetExFailed(_) => "SET_EX_FAILED",
            AppError::SetExpiryFailed(_) => "SET_EXPIRY_FAILED",
            AppError::GetFailed(_) => "GET_FAILED",
            AppError::MGetFailed(_) => "MGET_FAILED",
            AppError::DeleteFailed(_) => "DELETE_FAILED",
            AppError::SetHashFieldFailed(_) => "SETHASHFIELD_FAILED",
            AppError::GetHashFieldFailed(_) => "GETHASHFIELD_FAILED",
            AppError::RPushFailed(_) => "RPUSH_FAILED",
            AppError::RPopFailed(_) => "RPOP_FAILED",
            AppError::LPopFailed(_) => "LPOP_FAILED",
            AppError::LRangeFailed(_) => "LRANGE_FAILED",
            AppError::LLenFailed(_) => "LLEN_FAILED",
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::InvalidRedisEntryId(_) => "INVALID_REDIS_ENTRY_ID",
            AppError::RedisConnectionError => "REDIS_CONNECTION_FAILED",
            AppError::SubscribeError(_) => "SUBSCRIBE_FAILED",
            AppError::PublishError(_) => "PUBLISH_FAILED",
            AppError::GeoAddFailed(_) => "GEOADD_FAILED",
            AppError::ZAddFailed(_) => "ZADD_FAILED",
            AppError::ZremrangeByRankFailed(_) => "ZREMRANGEBYRANK_FAILED",
            AppError::GeoSearchFailed(_) => "GEOSEARCH_FAILED",
            AppError::ZCardFailed(_) => "ZCARD_FAILED",
            AppError::GeoPosFailed(_) => "GEOPOS_FAILED",
            AppError::ZRangeFailed(_) => "ZRANGE_FAILED",
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
            AppError::SetFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetExFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetExpiryFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GetFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::MGetFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DeleteFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetHashFieldFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GetHashFieldFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::InvalidRedisEntryId(_) => StatusCode::BAD_REQUEST,
            AppError::RedisConnectionError => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SubscribeError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::PublishError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GeoAddFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZAddFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZremrangeByRankFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GeoSearchFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZCardFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GeoPosFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ZRangeFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::RPushFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::RPopFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::LPopFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::LRangeFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::LLenFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
