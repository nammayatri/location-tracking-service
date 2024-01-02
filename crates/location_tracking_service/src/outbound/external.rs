/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use crate::common::types::*;
use crate::tools::callapi::{call_api, call_api_unwrapping_error};
use crate::tools::error::AppError;
use actix_http::StatusCode;
use reqwest::{Method, Url};

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
    merchant_id: &str,
) -> Result<AuthResponseData, AppError> {
    call_api_unwrapping_error::<AuthResponseData, String>(
        Method::GET,
        auth_url,
        vec![
            ("content-type", "application/json"),
            ("token", token),
            ("api-key", auth_api_key),
            ("merchant-id", merchant_id),
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
}
