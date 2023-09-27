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

#[derive(Debug, Serialize, thiserror::Error)]
pub enum AppError {
    #[error("InternalError")]
    InternalError(String),
    #[error("InvalidRequest")]
    InvalidRequest(String),
    #[error("InvalidRideStatus")]
    InvalidRideStatus(String),
    #[error("ExternalAPICallError")]
    ExternalAPICallError(String),
    #[error("SerializationError")]
    SerializationError(String),
    #[error("DeserializationError")]
    DeserializationError(String),
    #[error("DriverAppAuthFailed")]
    DriverAppAuthFailed(String),
    #[error("Unserviceable")]
    Unserviceable(f64, f64),
    #[error("HitsLimitExceeded")]
    HitsLimitExceeded,
    #[error("DriverBulkLocationUpdateFailed")]
    DriverBulkLocationUpdateFailed,
    #[error("InvalidConfiguration")]
    InvalidConfiguration(String),
    #[error("SetFailed")]
    SetFailed,
    #[error("SetExFailed")]
    SetExFailed,
    #[error("SetExpiryFailed")]
    SetExpiryFailed,
    #[error("GetFailed")]
    GetFailed,
    #[error("MGetFailed")]
    MGetFailed,
    #[error("DeleteFailed")]
    DeleteFailed,
    #[error("SetHashFieldFailed")]
    SetHashFieldFailed,
    #[error("GetHashFieldFailed")]
    GetHashFieldFailed,
    #[error("RPushFailed")]
    RPushFailed,
    #[error("RPopFailed")]
    RPopFailed,
    #[error("LPopFailed")]
    LPopFailed,
    #[error("LRangeFailed")]
    LRangeFailed,
    #[error("LLenFailed")]
    LLenFailed,
    #[error("NotFound")]
    NotFound,
    #[error("InvalidRedisEntryId")]
    InvalidRedisEntryId,
    #[error("RedisConnectionError")]
    RedisConnectionError,
    #[error("SubscribeError")]
    SubscribeError,
    #[error("PublishError")]
    PublishError,
    #[error("GeoAddFailed")]
    GeoAddFailed,
    #[error("ZAddFailed")]
    ZAddFailed,
    #[error("ZremrangeByRankFailed")]
    ZremrangeByRankFailed,
    #[error("GeoSearchFailed")]
    GeoSearchFailed,
    #[error("ZCardFailed")]
    ZCardFailed,
    #[error("GeoPosFailed")]
    GeoPosFailed,
    #[error("ZRangeFailed")]
    ZRangeFailed,
}

impl AppError {
    fn error_message(&self) -> ErrorBody {
        ErrorBody {
            error_message: self.variant_to_message(),
            error_code: self.variant_to_code(),
        }
    }

    fn variant_to_message(&self) -> String {
        match self {
            AppError::InternalError(err) => err.to_string(),
            AppError::InvalidRequest(err) => err.to_string(),
            AppError::InvalidRideStatus(ride_id) => {
                format!("Invalid Ride Status : RideId - {ride_id}")
            }
            AppError::ExternalAPICallError(err) => err.to_string(),
            AppError::SerializationError(err) => err.to_string(),
            AppError::DeserializationError(err) => err.to_string(),
            AppError::DriverAppAuthFailed(token) => format!("Invalid Token - {token}"),
            AppError::Unserviceable(lat, lon) => {
                format!("Location is unserviceable : (Lat : {lat}, Lon : {lon})")
            }
            _ => "".to_string(),
        }
    }

    fn variant_to_code(&self) -> String {
        match self {
            AppError::InternalError(_) => "INTERNAL_ERROR",
            AppError::InvalidRequest(_) => "INVALID_REQUEST",
            AppError::InvalidRideStatus(_) => "INVALID_RIDE_STATUS",
            AppError::ExternalAPICallError(_) => "EXTERNAL_API_CALL_ERROR",
            AppError::SerializationError(_) => "SERIALIZATION_ERROR",
            AppError::DeserializationError(_) => "DESERIALIZATION_ERROR",
            AppError::DriverAppAuthFailed(_) => "INVALID_TOKEN",
            AppError::Unserviceable(_, _) => "LOCATION_NOT_SERVICEABLE",
            AppError::HitsLimitExceeded => "HITS_LIMIT_EXCEED",
            AppError::DriverBulkLocationUpdateFailed => "DOBPP_BULK_LOCATION_UPDATE_FAILED",
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
            AppError::InvalidRideStatus(_) => StatusCode::BAD_REQUEST,
            AppError::ExternalAPICallError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DeserializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DriverAppAuthFailed(_) => StatusCode::UNAUTHORIZED,
            AppError::Unserviceable(_, _) => StatusCode::BAD_REQUEST,
            AppError::HitsLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            AppError::DriverBulkLocationUpdateFailed => StatusCode::INTERNAL_SERVER_ERROR,
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
