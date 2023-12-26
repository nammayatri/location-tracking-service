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
pub enum RedisError {
    SerializationError(String),
    DeserializationError(String),
    RedisConnectionError(String),
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
    XAddFailed(String),
    XReadFailed(String),
    XDeleteFailed(String),
}

impl RedisError {
    fn error_message(&self) -> ErrorBody {
        ErrorBody {
            error_message: self.message(),
            error_code: self.code(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            RedisError::SerializationError(err) => err.to_string(),
            RedisError::DeserializationError(err) => err.to_string(),
            RedisError::RedisConnectionError(err) => format!("Redis Connection Error : {err}"),
            RedisError::SetFailed(err) => format!("Redis Error : {err}"),
            RedisError::SetExFailed(err) => format!("Redis Error : {err}"),
            RedisError::SetExpiryFailed(err) => format!("Redis Error : {err}"),
            RedisError::GetFailed(err) => format!("Redis Error : {err}"),
            RedisError::MGetFailed(err) => format!("Redis Error : {err}"),
            RedisError::DeleteFailed(err) => format!("Redis Error : {err}"),
            RedisError::SetHashFieldFailed(err) => format!("Redis Error : {err}"),
            RedisError::GetHashFieldFailed(err) => format!("Redis Error : {err}"),
            RedisError::RPushFailed(err) => format!("Redis Error : {err}"),
            RedisError::RPopFailed(err) => format!("Redis Error : {err}"),
            RedisError::LPopFailed(err) => format!("Redis Error : {err}"),
            RedisError::LRangeFailed(err) => format!("Redis Error : {err}"),
            RedisError::LLenFailed(err) => format!("Redis Error : {err}"),
            RedisError::NotFound(err) => format!("Redis Error : {err}"),
            RedisError::InvalidRedisEntryId(err) => format!("Redis Error : {err}"),
            RedisError::SubscribeError(err) => format!("Redis Error : {err}"),
            RedisError::PublishError(err) => format!("Redis Error : {err}"),
            RedisError::GeoAddFailed(err) => format!("Redis Error : {err}"),
            RedisError::ZAddFailed(err) => format!("Redis Error : {err}"),
            RedisError::ZremrangeByRankFailed(err) => format!("Redis Error : {err}"),
            RedisError::GeoSearchFailed(err) => format!("Redis Error : {err}"),
            RedisError::ZCardFailed(err) => format!("Redis Error : {err}"),
            RedisError::GeoPosFailed(err) => format!("Redis Error : {err}"),
            RedisError::ZRangeFailed(err) => format!("Redis Error : {err}"),
            _ => "Some Error Occured".to_string(),
        }
    }

    fn code(&self) -> String {
        match self {
            RedisError::SerializationError(_) => "SERIALIZATION_ERROR",
            RedisError::DeserializationError(_) => "DESERIALIZATION_ERROR",
            RedisError::SetFailed(_) => "SET_FAILED",
            RedisError::SetExFailed(_) => "SET_EX_FAILED",
            RedisError::SetExpiryFailed(_) => "SET_EXPIRY_FAILED",
            RedisError::GetFailed(_) => "GET_FAILED",
            RedisError::MGetFailed(_) => "MGET_FAILED",
            RedisError::DeleteFailed(_) => "DELETE_FAILED",
            RedisError::SetHashFieldFailed(_) => "SETHASHFIELD_FAILED",
            RedisError::GetHashFieldFailed(_) => "GETHASHFIELD_FAILED",
            RedisError::RPushFailed(_) => "RPUSH_FAILED",
            RedisError::RPopFailed(_) => "RPOP_FAILED",
            RedisError::LPopFailed(_) => "LPOP_FAILED",
            RedisError::LRangeFailed(_) => "LRANGE_FAILED",
            RedisError::LLenFailed(_) => "LLEN_FAILED",
            RedisError::NotFound(_) => "NOT_FOUND",
            RedisError::InvalidRedisEntryId(_) => "INVALID_REDIS_ENTRY_ID",
            RedisError::RedisConnectionError(_) => "REDIS_CONNECTION_FAILED",
            RedisError::SubscribeError(_) => "SUBSCRIBE_FAILED",
            RedisError::PublishError(_) => "PUBLISH_FAILED",
            RedisError::GeoAddFailed(_) => "GEOADD_FAILED",
            RedisError::ZAddFailed(_) => "ZADD_FAILED",
            RedisError::ZremrangeByRankFailed(_) => "ZREMRANGEBYRANK_FAILED",
            RedisError::GeoSearchFailed(_) => "GEOSEARCH_FAILED",
            RedisError::ZCardFailed(_) => "ZCARD_FAILED",
            RedisError::GeoPosFailed(_) => "GEOPOS_FAILED",
            RedisError::ZRangeFailed(_) => "ZRANGE_FAILED",
            RedisError::XAddFailed(_) => "XADD_FAILED",
            RedisError::XReadFailed(_) => "XREAD_FAILED",
            RedisError::XDeleteFailed(_) => "XDEL_FAILED",
        }
        .to_string()
    }
}

impl ResponseError for RedisError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::json())
            .json(self.error_message())
    }

    fn status_code(&self) -> StatusCode {
        match self {
            RedisError::SerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::DeserializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::SetFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::SetExFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::SetExpiryFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::GetFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::MGetFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::DeleteFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::SetHashFieldFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::GetHashFieldFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::NotFound(_) => StatusCode::NOT_FOUND,
            RedisError::InvalidRedisEntryId(_) => StatusCode::BAD_REQUEST,
            RedisError::RedisConnectionError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::SubscribeError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::PublishError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::GeoAddFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::ZAddFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::ZremrangeByRankFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::GeoSearchFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::ZCardFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::GeoPosFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::ZRangeFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::RPushFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::RPopFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::LPopFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::LRangeFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::LLenFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::XAddFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::XReadFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            RedisError::XDeleteFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
