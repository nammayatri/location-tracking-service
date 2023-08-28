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
    #[error("InternalError: {0}")]
    InternalError(String),
    #[error("InternalServerError")]
    InternalServerError,
    #[error("SerializationError: {0}")]
    SerializationError(String),
    #[error("DeserializationError: {0}")]
    DeserializationError(String),
    #[error("Invalid Redis configuration")]
    DriverAppAuthFailed,
    #[error("Invalid Redis configuration")]
    Unserviceable,
    #[error("Invalid Redis configuration")]
    HitsLimitExceeded,
    #[error("Invalid Redis configuration")]
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
    #[error("Failed to append entry to Redis stream")]
    StreamAppendFailed,
    #[error("Failed to read queue from Redis stream")]
    StreamReadFailed,
    #[error("Failed to get stream length")]
    GetLengthFailed,
    #[error("Failed to delete queue from Redis stream")]
    StreamDeleteFailed,
    #[error("Failed to trim queue from Redis stream")]
    StreamTrimFailed,
    #[error("Failed to acknowledge Redis stream entry")]
    StreamAcknowledgeFailed,
    #[error("Failed to create Redis consumer group")]
    ConsumerGroupCreateFailed,
    #[error("Failed to destroy Redis consumer group")]
    ConsumerGroupDestroyFailed,
    #[error("Failed to delete consumer from consumer group")]
    ConsumerGroupRemoveConsumerFailed,
    #[error("Failed to set last ID on consumer group")]
    ConsumerGroupSetIdFailed,
    #[error("Failed to set Redis stream message owner")]
    ConsumerGroupClaimFailed,
    #[error("Failed to serialize application type to JSON")]
    JsonSerializationFailed,
    #[error("Failed to deserialize application type from JSON")]
    JsonDeserializationFailed,
    #[error("Failed to set hash in Redis")]
    SetHashFailed,
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
            AppError::InternalError(err) => ErrorBody {
                message: err.to_string(),
                code: "INTERNAL_ERROR".to_string(),
            },
            AppError::DriverAppAuthFailed => ErrorBody {
                message: format!("Authentication Failed With Driver Offer App").to_string(),
                code: "DRIVER_APP_AUTH_FAILED".to_string(),
            },
            _ => ErrorBody {
                message: self.to_string(),
                code: "ERROR".to_string(),
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
            AppError::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
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
            AppError::StreamAppendFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::StreamReadFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::GetLengthFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::StreamDeleteFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::StreamTrimFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::StreamAcknowledgeFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ConsumerGroupCreateFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ConsumerGroupDestroyFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ConsumerGroupRemoveConsumerFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ConsumerGroupSetIdFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ConsumerGroupClaimFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::JsonSerializationFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::JsonDeserializationFailed => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SetHashFailed => StatusCode::INTERNAL_SERVER_ERROR,
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
