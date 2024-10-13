/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use crate::common::types::*;
use crate::tools::error::AppError;
use actix_http::StatusCode;
use reqwest::{Method, Url};
use shared::tools::callapi::{call_api, call_api_unwrapping_error, Protocol};
use std::collections::HashMap;

/// Authenticates a driver using the `dobpp` method.
///
/// This function communicates with an external authentication service
/// to verify the driver's identity and authorization.
///
/// # Parameters
/// - `auth_url`: The endpoint URL for the authentication service.
/// - `token`: The authentication token for the driver.
/// - `auth_api_key`: The API key for the authentication service.
/// - `merchant_id`: The identifier for the merchant.
///
/// # Returns
/// - `Ok(AuthResponseData)`: Authentication was successful.
/// - `Err(AppError)`: An error occurred during authentication.
pub async fn authenticate_dobpp(
    auth_url: &Url,
    token: &str,
    auth_api_key: &str,
) -> Result<AuthResponseData, AppError> {
    call_api_unwrapping_error::<AuthResponseData, String, AppError>(
        Protocol::Http1,
        Method::GET,
        auth_url,
        vec![
            ("content-type", "application/json"),
            ("token", token),
            ("api-key", auth_api_key),
        ],
        None,
        Box::new(|resp| {
            if resp.status() == StatusCode::BAD_REQUEST {
                AppError::DriverAppAuthFailed
            } else if resp.status() == StatusCode::UNAUTHORIZED {
                AppError::DriverAppUnauthorized
            } else {
                AppError::DriverAppAuthFailed
            }
        }),
    )
    .await
}

/// Sends a bulk location update to the `dobpp` endpoint.
///
/// This function communicates with an external service to update the
/// location data for a given driver during a ride.
///
/// # Parameters
/// - `bulk_location_callback_url`: The endpoint URL for location updates.
/// - `ride_id`: The unique identifier for the ongoing ride.
/// - `driver_id`: The unique identifier for the driver.
/// - `on_ride_driver_locations`: A list of location points indicating the driver's route.
///
/// # Returns
/// - `Ok(APISuccess)`: The location update was successful.
/// - `Err(AppError)`: An error occurred during the bulk location update.
pub async fn bulk_location_update_dobpp(
    bulk_location_callback_url: &Url,
    ride_id: RideId,
    driver_id: DriverId,
    on_ride_driver_locations: Vec<Point>,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, BulkDataReq>(
        Protocol::Http1,
        Method::POST,
        bulk_location_callback_url,
        vec![("content-type", "application/json")],
        Some(BulkDataReq {
            ride_id,
            driver_id,
            loc: on_ride_driver_locations.clone(),
        }),
    )
    .await
    .map_err(|e| e.into())
}

pub async fn trigger_fcm_dobpp(
    trigger_fcm_callback_url: &Url,
    ride_id: RideId,
    driver_id: DriverId,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, TriggerFcmReq>(
        Protocol::Http1,
        Method::POST,
        trigger_fcm_callback_url,
        vec![("content-type", "application/json")],
        Some(TriggerFcmReq { ride_id, driver_id }),
    )
    .await
    .map_err(|e| e.into())
}

pub async fn trigger_stop_detection_event(
    stop_detection_callback_url: &Url,
    location: &Point,
    total_points: &usize,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, StopDetectionReq>(
        Protocol::Http1,
        Method::POST,
        stop_detection_callback_url,
        vec![("content-type", "application/json")],
        Some(StopDetectionReq {
            location: location.to_owned(),
            total_locations: *total_points,
        }),
    )
    .await
    .map_err(|e| e.into())
}

/**
 * vehicleServiceTier
 * vehicleNumber
 * startOtp
 * status: Pickup (10 mins, 5 mins, 3 mins) | Arrived | Inprogress (15 mins, 10 mins, 3 mins) | Completed
 */
/// Triggers a IOS live activity update for a vehicle ride during Pickup stage.
///
/// This function sends a notification to update the live activity feed
/// with the current status of a vehicle ride, including the type of vehicle,
/// vehicle number, ride start OTP, and remaining distance to the pickup point.
///
/// # Arguments
///
/// * `apns_topic` - The APNs topic used for the notification.
/// * `vehicle_type` - The type of vehicle involved in the ride.
/// * `vehicle_number` - The unique number of the vehicle.
/// * `ride_start_otp` - The OTP (One Time Password) used to start the ride.
/// * `estimated_pickup_distance` - The estimated distance to the pickup point in meters.
/// * `travelled_distance` - The distance travelled so far in meters.
///
pub async fn trigger_liveactivity(
    apns_url: &Url,
    vehicle_type: VehicleType,
    vehicle_number: String,
    ride_start_otp: u32,
    estimated_pickup_distance: Meters,
    travelled_distance: Meters,
) -> Result<(), AppError> {
    let pickup_distance = estimated_pickup_distance.inner() - travelled_distance.inner();

    let mut live_activity_payload = HashMap::new();
    live_activity_payload.insert("vehicleVariant".to_string(), vehicle_type.to_string());
    live_activity_payload.insert("vehicleNumber".to_string(), vehicle_number);
    live_activity_payload.insert("rideStartOtp".to_string(), ride_start_otp.to_string());
    live_activity_payload.insert(
        "pickupDistanceInMeters".to_string(),
        if pickup_distance > 0 {
            pickup_distance.to_string()
        } else {
            0.to_string()
        },
    );

    call_api::<(), TriggerApnsReq>(
        Protocol::Http2,
        Method::POST,
        apns_url,
        vec![
            ("content-type", "application/json"),
            ("Authorization", "---"),
            ("apns-push-type", "---"),
            ("apns-priority", "---"),
            ("apns-topic", "---"),
        ],
        Some(TriggerApnsReq {
            aps: Apns {
                timestamp: chrono::Utc::now().timestamp() as u64,
                event: "update".to_string(),
                content_state: live_activity_payload,
                alert: HashMap::new(),
            },
        }),
    )
    .await
    .map_err::<AppError, _>(|e| e.into())?;

    Ok(())
}
