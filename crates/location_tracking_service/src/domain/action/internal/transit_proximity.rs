/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::common::utils::distance_between_in_meters;
use crate::domain::types::internal::transit_proximity::*;
use crate::environment::AppState;
use crate::redis::commands::*;
use crate::tools::error::AppError;
use actix_web::web::Data;
use chrono::Utc;

/// Average walking speed in meters per second (5 km/h).
const WALKING_SPEED_MPS: f64 = 5.0 * 1000.0 / 3600.0;

/// Buffer time in seconds to account for walking variability.
const WALKING_BUFFER_SECONDS: i64 = 60;

pub async fn transit_proximity(
    data: Data<AppState>,
    request_body: TransitProximityRequest,
) -> Result<TransitProximityResponse, AppError> {
    // Step 1: Calculate walking distance and ETA from rider to target stop.
    let walking_distance =
        distance_between_in_meters(&request_body.rider_location, &request_body.target_stop_location);
    let walking_eta_seconds = (walking_distance / WALKING_SPEED_MPS).ceil() as i64;

    // Step 2: Look up tracked vehicles on the given route code using existing Redis infrastructure.
    let tracked_vehicles =
        get_route_location(&data.redis, &request_body.route_code).await?;

    // Step 3: Find the best (closest) vehicle heading toward the target stop.
    let vehicle_eta = find_best_vehicle_eta(
        &tracked_vehicles,
        &request_body.target_stop_location,
        &request_body.target_stop_code,
        &request_body.scheduled_departure,
    );

    // Step 4: Determine advisory based on walking ETA vs vehicle ETA.
    let (should_leave_now, advisory_message) =
        compute_advisory(walking_eta_seconds, &vehicle_eta);

    Ok(TransitProximityResponse {
        walking_eta_to_stop: walking_eta_seconds,
        walking_distance_to_stop: (walking_distance * 10.0).round() / 10.0,
        vehicle_eta,
        should_leave_now,
        advisory_message,
    })
}

/// Finds the vehicle with the best (lowest) ETA to the target stop from the tracked vehicles map.
fn find_best_vehicle_eta(
    tracked_vehicles: &std::collections::HashMap<String, VehicleTrackingInfo>,
    target_stop_location: &Point,
    target_stop_code: &str,
    scheduled_departure: &TimeStamp,
) -> Option<VehicleEta> {
    let now = Utc::now();

    let mut best: Option<(String, i64, i64, bool, TimeStamp)> = None;

    for (vehicle_id, info) in tracked_vehicles {
        let vehicle_location = Point {
            lat: info.latitude,
            lon: info.longitude,
        };

        // Check if the vehicle has upcoming stop information that includes the target stop.
        if let Some(ref upcoming_stops) = info.upcoming_stops {
            for upcoming_stop in upcoming_stops {
                if upcoming_stop.stop.stop_code == target_stop_code
                    && upcoming_stop.status == UpcomingStopStatus::Upcoming
                {
                    let eta_seconds = upcoming_stop
                        .eta
                        .inner()
                        .signed_duration_since(now)
                        .num_seconds();

                    if eta_seconds < 0 {
                        continue;
                    }

                    let delay_seconds = eta_seconds
                        - scheduled_departure
                            .inner()
                            .signed_duration_since(now)
                            .num_seconds();

                    let last_updated = info.timestamp.unwrap_or(TimeStamp(now));

                    if best.is_none() || eta_seconds < best.as_ref().unwrap().1 {
                        best = Some((
                            vehicle_id.clone(),
                            eta_seconds,
                            delay_seconds,
                            true,
                            last_updated,
                        ));
                    }
                    break;
                }
            }
        }

        // If no upcoming stop match, fall back to distance-based ETA estimation.
        if best.is_none() || !best.as_ref().unwrap().3 {
            let distance_to_stop =
                distance_between_in_meters(&vehicle_location, target_stop_location);

            // Estimate vehicle speed: use reported speed or default to 20 km/h for buses.
            let speed_mps = info
                .speed
                .map(|s| {
                    let SpeedInMeterPerSecond(v) = s;
                    if v > 0.5 { v } else { 20.0 * 1000.0 / 3600.0 }
                })
                .unwrap_or(20.0 * 1000.0 / 3600.0);

            let eta_seconds = (distance_to_stop / speed_mps).ceil() as i64;

            let scheduled_eta_seconds = scheduled_departure
                .inner()
                .signed_duration_since(now)
                .num_seconds();
            let delay_seconds = eta_seconds - scheduled_eta_seconds.max(0);

            let last_updated = info.timestamp.unwrap_or(TimeStamp(now));
            let is_live = info.timestamp.is_some();

            if best.is_none() || eta_seconds < best.as_ref().unwrap().1 {
                best = Some((
                    vehicle_id.clone(),
                    eta_seconds,
                    delay_seconds,
                    is_live,
                    last_updated,
                ));
            }
        }
    }

    best.map(
        |(vehicle_id, eta_to_stop_seconds, current_delay_seconds, is_live, last_updated)| {
            VehicleEta {
                vehicle_id,
                eta_to_stop_seconds,
                current_delay_seconds,
                is_live,
                last_updated,
            }
        },
    )
}

/// Computes advisory message and whether the rider should leave now.
fn compute_advisory(walking_eta_seconds: i64, vehicle_eta: &Option<VehicleEta>) -> (bool, String) {
    match vehicle_eta {
        Some(eta) => {
            let walking_arrival = walking_eta_seconds + WALKING_BUFFER_SECONDS;
            if walking_arrival >= eta.eta_to_stop_seconds {
                (
                    true,
                    "Leave now to reach the stop before the bus arrives".to_string(),
                )
            } else {
                let wait_seconds = eta.eta_to_stop_seconds - walking_arrival;
                if wait_seconds <= 120 {
                    (
                        true,
                        "Leave now, the bus will arrive shortly after you reach the stop"
                            .to_string(),
                    )
                } else {
                    let wait_minutes = wait_seconds / 60;
                    (
                        false,
                        format!(
                            "You can wait about {} minutes before leaving for the stop",
                            wait_minutes
                        ),
                    )
                }
            }
        }
        None => {
            (
                true,
                "No live vehicle data available, leave now to be safe".to_string(),
            )
        }
    }
}
