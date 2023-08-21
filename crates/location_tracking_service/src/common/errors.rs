use std::time::Duration;
use strum_macros::Display;
use serde::Serialize;
use actix_web::{ResponseError, HttpResponse, http::{StatusCode, header::ContentType}};

#[derive(Debug, Serialize)]
struct ErrorBody {
    message : String,
    code : String
}

#[derive(Debug, Display)]
pub enum AppError {
    InternalServerError,
    DriverAppAuthFailed(Duration),
    Unserviceable,
    HitsLimitExceeded,
    DriverBulkLocationUpdateFailed,
}

impl AppError {
    fn error_message(&self) -> ErrorBody {
        match self {
            AppError::DriverAppAuthFailed(_duration) => ErrorBody { message : format!("Authentication Failed With Driver Offer App").to_string(), code : "DRIVER_APP_AUTH_FAILED".to_string() },
            _ => ErrorBody { message: "Error Occured!".to_string(), code: "ERROR".to_string() }
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
            AppError::DriverAppAuthFailed(_) => StatusCode::UNAUTHORIZED,
            AppError::Unserviceable => StatusCode::BAD_REQUEST,
            AppError::HitsLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            AppError::DriverBulkLocationUpdateFailed => StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}