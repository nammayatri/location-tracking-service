//!
//! Errors specific to this custom redis interface
//!
use actix_web::{
    http::{header::ContentType, StatusCode},
    HttpResponse, ResponseError,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct ErrorBody {
    message: String,
    code: String,
}

#[derive(Debug, Serialize, thiserror::Error)]
pub enum AppError {
    #[error("Internal Server Error: {0}")]
    InternalError(String),
    #[error("Invalid Ride Status : RideId - {0}")]
    InvalidRideStatus(String),
    #[error("External Call API Error: {0}")]
    ExternalAPICallError(String),
    #[error("Json Serialization Error: {0}")]
    SerializationError(String),
    #[error("Json Deserialization Error: {0}")]
    DeserializationError(String),
    #[error("Authentication failed with driver app")]
    DriverAppAuthFailed,
    #[error("Location is unserviceable")]
    Unserviceable,
    #[error("Hits limit exceeded")]
    HitsLimitExceeded,
    #[error("Driver bulk location update failed")]
    DriverBulkLocationUpdateFailed,
    #[error("Invalid Redis configuration: {0}")]
    InvalidConfiguration(String),
    #[error("Failed to set key value in Redis")]
    SetFailed,
    #[error("Failed to set key value with expiry in Redis")]
    SetExFailed,
    #[error("Failed to set expiry for key value in Redis")]
    SetExpiryFailed,
    #[error("Failed to get key value in Redis")]
    GetFailed,
    #[error("Failed to delete key value in Redis")]
    DeleteFailed,
    #[error("Failed to set hash field in Redis")]
    SetHashFieldFailed,
    #[error("Failed to get hash field in Redis")]
    GetHashFieldFailed,
    #[error("The requested value was not found in Redis")]
    NotFound,
    #[error("Invalid RedisEntryId provided")]
    InvalidRedisEntryId,
    #[error("Failed to establish Redis connection")]
    RedisConnectionError,
    #[error("Failed to subscribe to a channel")]
    SubscribeError,
    #[error("Failed to publish to a channel")]
    PublishError,
    #[error("Failed to add geospatial items to Redis")]
    GeoAddFailed,
    #[error("Failed to zadd from Redis")]
    ZAddFailed,
    #[error("Failed to zremrangebyrank from Redis")]
    ZremrangeByRankFailed,
    #[error("Failed to geo search from Redis")]
    GeoSearchFailed,
    #[error("Failed to zcard from Redis")]
    ZCardFailed,
    #[error("Failed to get geospatial values from Redis")]
    GeoPosFailed,
    #[error("Failed to get array of members from Redis")]
    ZRangeFailed,
}

impl AppError {
    fn error_message(&self) -> ErrorBody {
        match self {
            _ => ErrorBody {
                message: self.to_string(),
                code: self.to_string(),
            },
        }
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
            AppError::InvalidRideStatus(_) => StatusCode::BAD_REQUEST,
            AppError::ExternalAPICallError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DeserializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DriverAppAuthFailed => StatusCode::UNAUTHORIZED,
            AppError::Unserviceable => StatusCode::BAD_REQUEST,
            AppError::HitsLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            AppError::DriverBulkLocationUpdateFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InvalidConfiguration(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetExFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetExpiryFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GetFailed => StatusCode::INTERNAL_SERVER_ERROR,
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
        }
    }
}
