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
    PanicOccured(String),
    DriverRideDetailsNotFound,
    DriverLastKnownLocationNotFound,
    DriverLastLocationTimestampNotFound,
    UnprocessibleRequest(String),
    LargePayloadSize(usize, usize),
    InvalidRideStatus(String, String),
    ExternalAPICallError(String),
    SerializationError(String),
    DeserializationError(String),
    Unserviceable(f64, f64),
    HitsLimitExceeded(String),
    DriverBulkLocationUpdateFailed(String),
    InvalidConfiguration(String),
    RequestTimeout,
    DriverAppUnauthorized,
    DriverAppAuthFailed,
    KafkaPushFailed(String),
    DrainerPushFailed(String),
    CacConfigFailed(String),
    DefaultConfigsNotFound(String),
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
            AppError::LargePayloadSize(length, limit) => {
                format!("Content length ({length} Bytes) greater than allowed maximum limit : ({limit} Bytes)")
            }
            AppError::DriverBulkLocationUpdateFailed(err) => {
                format!("Driver Bulk Location Update Failed : {err}")
            }
            AppError::Unserviceable(lat, lon) => {
                format!("Location is unserviceable : (Lat : {lat}, Lon : {lon})")
            }
            AppError::PanicOccured(reason) => {
                format!("Panic occured : {reason}")
            }
            AppError::CacConfigFailed(reason) => {
                format!("CAC Config Failed : {reason}")
            }
            AppError::DefaultConfigsNotFound(reason) => {
                format!("Default Configs Not Found : {reason}")
            }
            _ => "Some Error Occured".to_string(),
        }
    }

    fn code(&self) -> String {
        match self {
            AppError::InternalError(_) => "INTERNAL_ERROR",
            AppError::PanicOccured(_) => "PANIC_OCCURED",
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
            AppError::DriverAppUnauthorized => "INVALID_TOKEN",
            AppError::DriverAppAuthFailed => "INVALID_REQUEST",
            AppError::Unserviceable(_, _) => "LOCATION_NOT_SERVICEABLE",
            AppError::LargePayloadSize(_, _) => "LARGE_PAYLOAD_SIZE",
            AppError::HitsLimitExceeded(_) => "HITS_LIMIT_EXCEED",
            AppError::DriverBulkLocationUpdateFailed(_) => "DOBPP_BULK_LOCATION_UPDATE_FAILED",
            AppError::InvalidConfiguration(_) => "INVALID_REDIS_CONFIGURATION",
            AppError::RequestTimeout => "REQUEST_TIMEOUT",
            AppError::KafkaPushFailed(_) => "KAFKA_PUSH_FAILED",
            AppError::DrainerPushFailed(_) => "DRAINER_PUSH_FAILED",
            AppError::CacConfigFailed(_) => "CAC_CONFIG_FAILED",
            AppError::DefaultConfigsNotFound(_) => "DEFAULT_CONFIGS_NOT_FOUND",
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
            AppError::PanicOccured(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            AppError::DriverRideDetailsNotFound => StatusCode::BAD_REQUEST,
            AppError::DriverLastKnownLocationNotFound => StatusCode::BAD_REQUEST,
            AppError::DriverLastLocationTimestampNotFound => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::UnprocessibleRequest(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::InvalidRideStatus(_, _) => StatusCode::BAD_REQUEST,
            AppError::ExternalAPICallError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::SerializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DeserializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DriverAppUnauthorized => StatusCode::UNAUTHORIZED,
            AppError::DriverAppAuthFailed => StatusCode::BAD_REQUEST,
            AppError::Unserviceable(_, _) => StatusCode::BAD_REQUEST,
            AppError::HitsLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::LargePayloadSize(_, _) => StatusCode::PAYLOAD_TOO_LARGE,
            AppError::DriverBulkLocationUpdateFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::KafkaPushFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DrainerPushFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::CacConfigFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::DefaultConfigsNotFound(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::InvalidConfiguration(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
        }
    }
}
