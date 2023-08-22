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
    #[error("InternalServerError")]
    InternalServerError,
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
    #[error("Failed to read entries from Redis stream")]
    StreamReadFailed,
    #[error("Failed to get stream length")]
    GetLengthFailed,
    #[error("Failed to delete entries from Redis stream")]
    StreamDeleteFailed,
    #[error("Failed to trim entries from Redis stream")]
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
            AppError::DriverAppAuthFailed => ErrorBody {
                message: format!("Authentication Failed With Driver Offer App").to_string(),
                code: "DRIVER_APP_AUTH_FAILED".to_string(),
            },
            _ => ErrorBody {
                message: "Error Occured!".to_string(),
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
            AppError::InternalServerError => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DriverAppAuthFailed => StatusCode::UNAUTHORIZED,
            AppError::Unserviceable => StatusCode::BAD_REQUEST,
            AppError::HitsLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            AppError::DriverBulkLocationUpdateFailed => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
