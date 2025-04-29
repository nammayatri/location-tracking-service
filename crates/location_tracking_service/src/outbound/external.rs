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

pub async fn trigger_fcm_bap(
    trigger_fcm_callback_url_bap: &Url,
    ride_id: RideId,
    driver_id: DriverId,
    ride_notification_status: RideNotificationStatus,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, TriggerStatusFcmReq>(
        Protocol::Http1,
        Method::POST,
        trigger_fcm_callback_url_bap,
        vec![("content-type", "application/json")],
        Some(TriggerStatusFcmReq {
            ride_id,
            driver_id,
            ride_notification_status,
        }),
    )
    .await
    .map_err(|e| e.into())
}

pub async fn trigger_stop_detection_event(
    stop_detection_callback_url: &Url,
    location: &Point,
    ride_id: RideId,
    driver_id: DriverId,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, StopDetectionReq>(
        Protocol::Http1,
        Method::POST,
        stop_detection_callback_url,
        vec![("content-type", "application/json")],
        Some(StopDetectionReq {
            location: location.to_owned(),
            ride_id,
            driver_id,
        }),
    )
    .await
    .map_err(|e| e.into())
}

pub async fn driver_reached_destination(
    driver_reached_destination_callback_url: &Url,
    location: &Point,
    ride_id: RideId,
    driver_id: DriverId,
    vehicle_type: VehicleType,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, DriverReachedDestinationReq>(
        Protocol::Http1,
        Method::POST,
        driver_reached_destination_callback_url,
        vec![("content-type", "application/json")],
        Some(DriverReachedDestinationReq {
            location: location.to_owned(),
            ride_id,
            driver_id,
            vehicle_variant: vehicle_type,
        }),
    )
    .await
    .map_err(|e| e.into())
}

pub async fn driver_source_departed(
    driver_source_departed_callback_url: &Url,
    location: &Point,
    ride_id: RideId,
    driver_id: DriverId,
    vehicle_type: VehicleType,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, DriverSourceDepartedReq>(
        Protocol::Http1,
        Method::POST,
        driver_source_departed_callback_url,
        vec![("content-type", "application/json")],
        Some(DriverSourceDepartedReq {
            location: location.to_owned(),
            ride_id,
            driver_id,
            vehicle_variant: vehicle_type,
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

pub async fn trigger_detection_alert(
    alert_url: &Url,
    unified_alert_req: ViolationDetectionReq,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, ViolationDetectionReq>(
        Protocol::Http1,
        Method::POST,
        alert_url,
        vec![("content-type", "application/json")],
        Some(unified_alert_req),
    )
    .await
    .map_err(|e| e.into())
}

/// Computes routes between two points using the Google Routes API.
///
/// This function communicates with the Google Routes API to calculate
/// the optimal route between an origin and destination point.
///
/// # Parameters
/// - `routes_url`: The Google Routes API endpoint URL.
/// - `api_key`: The Google API key for authentication.
/// - `origin`: The starting point coordinates.
/// - `destination`: The ending point coordinates.
/// - `intermediates`: Optional intermediate points along the route.
/// - `travel_mode`: The mode of travel (e.g., Drive, Walk, Bicycle).
///
/// # Returns
/// - `Ok(GoogleRoutesResponse)`: The route computation was successful.
/// - `Err(AppError)`: An error occurred during route computation.
#[allow(clippy::too_many_arguments)]
pub async fn compute_routes(
    routes_url: &Url,
    api_key: &str,
    origin: &Point,
    destination: &Point,
    intermediates: Vec<Point>,
    travel_mode: TravelMode,
) -> Result<GoogleRoutesResponse, AppError> {
    let request = GoogleRoutesRequest {
        origin: Location {
            location: OuterLatLng {
                lat_lng: LatLng {
                    latitude: origin.lat.inner(),
                    longitude: origin.lon.inner(),
                },
            },
        },
        destination: Location {
            location: OuterLatLng {
                lat_lng: LatLng {
                    latitude: destination.lat.inner(),
                    longitude: destination.lon.inner(),
                },
            },
        },
        intermediates: intermediates
            .into_iter()
            .map(|point| Location {
                location: OuterLatLng {
                    lat_lng: LatLng {
                        latitude: point.lat.inner(),
                        longitude: point.lon.inner(),
                    },
                },
            })
            .collect(),
        travel_mode,
        routing_preference: "TRAFFIC_AWARE".to_string(),
        compute_alternative_routes: false,
        extra_computations: vec![],
    };

    call_api::<GoogleRoutesResponse, GoogleRoutesRequest>(
        Protocol::Http1,
        Method::POST,
        routes_url,
        vec![
            ("content-type", "application/json"),
            ("X-Goog-Api-Key", api_key),
            ("X-Goog-FieldMask", "routes.legs.*,routes.distanceMeters,routes.duration,routes.viewport.*,routes.polyline.*,routes.routeLabels.*"),
        ],
        Some(request),
    )
    .await
    .map_err(|e| e.into())
}

/// Call the OSRM Distance Matrix API to get distances and durations between multiple points
pub async fn get_distance_matrix(
    sources: &[Point],
    destinations: &[Point],
    osrm_distance_matrix_base_url: &Url,
) -> Result<OsrmDistanceMatrixResponse, AppError> {
    let mut coordinates = String::new();
    for point in sources.iter().chain(destinations.iter()) {
        if !coordinates.is_empty() {
            coordinates.push(';');
        }
        coordinates.push_str(&format!(
            "{:.6},{:.6}",
            point.lon.inner(),
            point.lat.inner()
        ));
    }

    let url = format!(
        "{}table/v1/driving/{}?annotations=duration,distance&sources={}&destinations={}",
        osrm_distance_matrix_base_url.as_str(),
        coordinates,
        (0..sources.len())
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join(";"),
        (sources.len()..sources.len() + destinations.len())
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join(";")
    );

    let url =
        Url::parse(&url).map_err(|e| AppError::InvalidRequest(format!("Invalid URL: {}", e)))?;

    call_api::<OsrmDistanceMatrixResponse, ()>(
        Protocol::Http1,
        Method::GET,
        &url,
        vec![("content-type", "application/json")],
        None,
    )
    .await
    .map_err(|e| e.into())
}
