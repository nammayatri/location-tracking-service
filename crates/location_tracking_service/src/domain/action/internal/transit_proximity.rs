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
use tracing::{info, warn};

/// Average walking speed in meters per second (5 km/h).
const WALKING_SPEED_MPS: f64 = 5.0 * 1000.0 / 3600.0;

/// Buffer time in seconds to account for walking variability.
const WALKING_BUFFER_SECONDS: i64 = 60;

/// Maximum reasonable latitude value.
const MAX_LATITUDE: f64 = 90.0;

/// Maximum reasonable longitude value.
const MAX_LONGITUDE: f64 = 180.0;

/// Maximum age in seconds for a vehicle's timestamp to be considered fresh.
const VEHICLE_FRESHNESS_THRESHOLD_SECONDS: i64 = 300;

/// Validates the transit proximity request fields.
fn validate_request(request_body: &TransitProximityRequest) -> Result<(), AppError> {
    let Latitude(rider_lat) = request_body.rider_location.lat;
    let Longitude(rider_lon) = request_body.rider_location.lon;
    let Latitude(stop_lat) = request_body.target_stop_location.lat;
    let Longitude(stop_lon) = request_body.target_stop_location.lon;

    if rider_lat.abs() > MAX_LATITUDE || stop_lat.abs() > MAX_LATITUDE {
        return Err(AppError::InvalidRequest(format!(
            "Latitude out of range [-90, 90]: rider_lat={}, stop_lat={}",
            rider_lat, stop_lat
        )));
    }

    if rider_lon.abs() > MAX_LONGITUDE || stop_lon.abs() > MAX_LONGITUDE {
        return Err(AppError::InvalidRequest(format!(
            "Longitude out of range [-180, 180]: rider_lon={}, stop_lon={}",
            rider_lon, stop_lon
        )));
    }

    if request_body.route_code.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "route_code must not be empty".to_string(),
        ));
    }

    if request_body.target_stop_code.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "target_stop_code must not be empty".to_string(),
        ));
    }

    if request_body.gtfs_id.trim().is_empty() {
        return Err(AppError::InvalidRequest(
            "gtfs_id must not be empty".to_string(),
        ));
    }

    Ok(())
}

pub async fn transit_proximity(
    data: Data<AppState>,
    request_body: TransitProximityRequest,
) -> Result<TransitProximityResponse, AppError> {
    // Step 0: Validate input fields.
    validate_request(&request_body)?;

    info!(
        route_code = %request_body.route_code,
        stop_code = %request_body.target_stop_code,
        gtfs_id = %request_body.gtfs_id,
        "Processing transit proximity request"
    );

    // Step 1: Calculate walking distance and ETA from rider to target stop.
    let walking_distance = distance_between_in_meters(
        &request_body.rider_location,
        &request_body.target_stop_location,
    );
    let walking_eta_seconds = (walking_distance / WALKING_SPEED_MPS).ceil() as i64;

    info!(
        walking_distance_m = walking_distance,
        walking_eta_s = walking_eta_seconds,
        "Computed walking ETA"
    );

    // Step 2: Look up tracked vehicles on the given route code using existing Redis infrastructure.
    let mut data_quality: Option<String> = None;
    let tracked_vehicles = match get_route_location(&data.redis, &request_body.route_code).await {
        Ok(vehicles) => vehicles,
        Err(err) => {
            warn!(
                route_code = %request_body.route_code,
                error = %err,
                data_quality = "degraded",
                "Failed to fetch tracked vehicles from Redis, proceeding with empty vehicle set"
            );
            data_quality = Some("degraded".to_string());
            std::collections::HashMap::new()
        }
    };

    if tracked_vehicles.is_empty() {
        info!(
            route_code = %request_body.route_code,
            "No tracked vehicles found for route"
        );
    } else {
        info!(
            route_code = %request_body.route_code,
            vehicle_count = tracked_vehicles.len(),
            "Found tracked vehicles for route"
        );
    }

    // Step 3: Find the best (closest) vehicle heading toward the target stop.
    let vehicle_eta = find_best_vehicle_eta(
        &tracked_vehicles,
        &request_body.target_stop_location,
        &request_body.target_stop_code,
        &request_body.scheduled_departure,
    );

    if let Some(ref eta) = vehicle_eta {
        info!(
            vehicle_id = %eta.vehicle_id,
            eta_to_stop_s = eta.eta_to_stop_seconds,
            delay_s = eta.current_delay_seconds,
            is_live = eta.is_live,
            "Best vehicle ETA determined"
        );
    } else {
        warn!(
            route_code = %request_body.route_code,
            stop_code = %request_body.target_stop_code,
            "No vehicle ETA could be determined"
        );
    }

    // Step 4: Determine advisory based on walking ETA vs vehicle ETA.
    let (should_leave_now, advisory_message) = compute_advisory(walking_eta_seconds, &vehicle_eta);

    info!(
        should_leave_now = should_leave_now,
        advisory = %advisory_message,
        "Transit proximity advisory computed"
    );

    Ok(TransitProximityResponse {
        walking_eta_to_stop: walking_eta_seconds,
        walking_distance_to_stop: (walking_distance * 10.0).round() / 10.0,
        vehicle_eta,
        should_leave_now,
        advisory_message,
        data_quality,
    })
}

/// Finds the vehicle with the best (lowest) ETA to the target stop from the tracked vehicles map.
///
/// Upcoming-stop-based ETAs are always preferred over distance-based estimates because
/// they come from real-time schedule data and are more accurate. Distance-based estimates
/// are only used when no vehicle has upcoming stop information for the target stop.
fn find_best_vehicle_eta(
    tracked_vehicles: &std::collections::HashMap<String, VehicleTrackingInfo>,
    target_stop_location: &Point,
    target_stop_code: &str,
    scheduled_departure: &TimeStamp,
) -> Option<VehicleEta> {
    let now = Utc::now();

    // Track the best upcoming-stop-based match and the best distance-based match separately.
    // Fields: (vehicle_id, eta_seconds, delay_seconds, is_live, last_updated)
    let mut best_stop_based: Option<(String, i64, i64, bool, TimeStamp)> = None;
    let mut best_distance_based: Option<(String, i64, i64, bool, TimeStamp)> = None;

    for (vehicle_id, info) in tracked_vehicles {
        // Fix 1: Skip vehicles that are not live (no timestamp).
        let last_updated = match info.timestamp {
            Some(ts) => ts,
            None => {
                warn!(
                    vehicle_id = %vehicle_id,
                    "Skipping vehicle with no timestamp (not live)"
                );
                continue;
            }
        };

        // Fix 1: Skip vehicles whose timestamp is stale (older than freshness threshold).
        let age_seconds = now
            .signed_duration_since(last_updated.inner())
            .num_seconds();
        if age_seconds > VEHICLE_FRESHNESS_THRESHOLD_SECONDS {
            warn!(
                vehicle_id = %vehicle_id,
                age_seconds = age_seconds,
                "Skipping stale vehicle (older than {} seconds)",
                VEHICLE_FRESHNESS_THRESHOLD_SECONDS
            );
            continue;
        }

        // Fix 2: Skip vehicles where the target stop has already been reached.
        if let Some(ref upcoming_stops) = info.upcoming_stops {
            let target_reached = upcoming_stops.iter().any(|us| {
                us.stop.stop_code == target_stop_code && us.status == UpcomingStopStatus::Reached
            });
            if target_reached {
                warn!(
                    vehicle_id = %vehicle_id,
                    stop_code = %target_stop_code,
                    "Skipping vehicle that has already passed the target stop"
                );
                continue;
            }
        }

        let vehicle_location = Point {
            lat: info.latitude,
            lon: info.longitude,
        };

        let mut found_upcoming_stop_match = false;

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

                    // Fix 2: Skip vehicles with negative ETA (already passed the stop).
                    if eta_seconds < 0 {
                        continue;
                    }

                    let delay_seconds = eta_seconds
                        - scheduled_departure
                            .inner()
                            .signed_duration_since(now)
                            .num_seconds();

                    if best_stop_based.is_none()
                        || eta_seconds < best_stop_based.as_ref().unwrap().1
                    {
                        best_stop_based = Some((
                            vehicle_id.clone(),
                            eta_seconds,
                            delay_seconds,
                            true,
                            last_updated,
                        ));
                    }
                    found_upcoming_stop_match = true;
                    break;
                }
            }
        }

        // If no upcoming stop match for this vehicle, fall back to distance-based ETA estimation.
        if !found_upcoming_stop_match {
            let distance_to_stop =
                distance_between_in_meters(&vehicle_location, target_stop_location);

            // Estimate vehicle speed: use reported speed or default to 20 km/h for buses.
            let speed_mps = info
                .speed
                .map(|s| {
                    let SpeedInMeterPerSecond(v) = s;
                    if v > 0.5 {
                        v
                    } else {
                        20.0 * 1000.0 / 3600.0
                    }
                })
                .unwrap_or(20.0 * 1000.0 / 3600.0);

            let eta_seconds = (distance_to_stop / speed_mps).ceil() as i64;

            let scheduled_eta_seconds = scheduled_departure
                .inner()
                .signed_duration_since(now)
                .num_seconds();
            let delay_seconds = eta_seconds - scheduled_eta_seconds;

            let is_live = info.timestamp.is_some();

            if best_distance_based.is_none()
                || eta_seconds < best_distance_based.as_ref().unwrap().1
            {
                best_distance_based = Some((
                    vehicle_id.clone(),
                    eta_seconds,
                    delay_seconds,
                    is_live,
                    last_updated,
                ));
            }
        }
    }

    // Prefer upcoming-stop-based matches over distance-based estimates.
    let best = best_stop_based.or(best_distance_based);

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
        None => (
            true,
            "No live vehicle data available, leave now to be safe".to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use std::collections::HashMap;

    // ── Helper builders ─────────────────────────────────────────────────

    fn make_point(lat: f64, lon: f64) -> Point {
        Point {
            lat: Latitude(lat),
            lon: Longitude(lon),
        }
    }

    fn make_stop_obj(stop_code: &str) -> Stop {
        Stop {
            name: format!("Stop {}", stop_code),
            stop_code: stop_code.to_string(),
            coordinate: make_point(12.97, 77.59),
            stop_idx: 0,
            distance_to_upcoming_intermediate_stop: Meters(0),
            duration_to_upcoming_intermediate_stop: Seconds(0),
            distance_from_previous_intermediate_stop: Meters(0),
            stop_type: StopType::UpcomingStop,
        }
    }

    fn make_upcoming_stop(
        stop_code: &str,
        eta_offset_secs: i64,
        status: UpcomingStopStatus,
    ) -> UpcomingStop {
        UpcomingStop {
            stop: make_stop_obj(stop_code),
            eta: TimeStamp(Utc::now() + Duration::seconds(eta_offset_secs)),
            status,
            delta: 0.0,
        }
    }

    fn make_vehicle(
        lat: f64,
        lon: f64,
        speed: Option<f64>,
        timestamp: Option<chrono::DateTime<Utc>>,
        upcoming_stops: Option<Vec<UpcomingStop>>,
    ) -> VehicleTrackingInfo {
        VehicleTrackingInfo {
            start_time: None,
            schedule_relationship: None,
            trip_id: None,
            latitude: Latitude(lat),
            longitude: Longitude(lon),
            speed: speed.map(SpeedInMeterPerSecond),
            timestamp: timestamp.map(TimeStamp),
            ride_status: None,
            upcoming_stops,
        }
    }

    // ── compute_advisory tests ──────────────────────────────────────────

    #[test]
    fn test_advisory_no_vehicle_data() {
        let (should_leave, msg) = compute_advisory(300, &None);
        assert!(should_leave);
        assert!(msg.contains("No live vehicle data"));
    }

    #[test]
    fn test_advisory_walking_takes_longer_than_vehicle() {
        // Walking + buffer = 360s, vehicle arrives in 200s. Rider should leave now.
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 200,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, msg) = compute_advisory(300, &Some(eta));
        assert!(should_leave);
        assert!(msg.contains("Leave now to reach the stop before the bus arrives"));
    }

    #[test]
    fn test_advisory_bus_arrives_shortly_after_walking() {
        // Walking + buffer = 360s, vehicle arrives in 400s. wait_seconds = 40 <= 120.
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 400,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, msg) = compute_advisory(300, &Some(eta));
        assert!(should_leave);
        assert!(msg.contains("shortly after"));
    }

    #[test]
    fn test_advisory_can_wait() {
        // Walking + buffer = 360s, vehicle arrives in 900s. wait = 540s > 120.
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 900,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, msg) = compute_advisory(300, &Some(eta));
        assert!(!should_leave);
        assert!(msg.contains("wait about 9 minutes"));
    }

    #[test]
    fn test_advisory_boundary_wait_exactly_120() {
        // Walking + buffer = 360s, vehicle arrives at 480s. wait = 120. <= 120 -> leave now.
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 480,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, _msg) = compute_advisory(300, &Some(eta));
        assert!(should_leave);
    }

    #[test]
    fn test_advisory_boundary_wait_121() {
        // Walking + buffer = 360s, vehicle at 481s. wait = 121 > 120 -> can wait.
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 481,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, _msg) = compute_advisory(300, &Some(eta));
        assert!(!should_leave);
    }

    // ── find_best_vehicle_eta tests ─────────────────────────────────────

    #[test]
    fn test_no_tracked_vehicles() {
        let vehicles: HashMap<String, VehicleTrackingInfo> = HashMap::new();
        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));
        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_none());
    }

    #[test]
    fn test_vehicle_with_upcoming_stop_match() {
        let mut vehicles = HashMap::new();
        let upcoming = vec![make_upcoming_stop(
            "STOP_X",
            300,
            UpcomingStopStatus::Upcoming,
        )];
        vehicles.insert(
            "bus_1".to_string(),
            make_vehicle(12.96, 77.58, Some(5.0), Some(Utc::now()), Some(upcoming)),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        let eta = result.unwrap();
        assert_eq!(eta.vehicle_id, "bus_1");
        assert!(eta.is_live);
        // ETA should be approximately 300 seconds (with some time passing tolerance).
        assert!((eta.eta_to_stop_seconds - 300).abs() <= 2);
    }

    #[test]
    fn test_vehicle_with_upcoming_stop_already_reached() {
        let mut vehicles = HashMap::new();
        let upcoming = vec![make_upcoming_stop(
            "STOP_X",
            300,
            UpcomingStopStatus::Reached,
        )];
        vehicles.insert(
            "bus_1".to_string(),
            make_vehicle(12.96, 77.58, Some(5.0), Some(Utc::now()), Some(upcoming)),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        // The stop status is Reached, so the vehicle should be excluded entirely.
        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_none());
    }

    #[test]
    fn test_vehicle_with_negative_eta_skipped() {
        let mut vehicles = HashMap::new();
        // ETA is in the past (negative offset).
        let upcoming = vec![make_upcoming_stop(
            "STOP_X",
            -60,
            UpcomingStopStatus::Upcoming,
        )];
        vehicles.insert(
            "bus_1".to_string(),
            make_vehicle(12.96, 77.58, Some(5.0), Some(Utc::now()), Some(upcoming)),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        // Negative ETA should be skipped; falls back to distance-based.
        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        // Since we fell back to distance-based, is_live depends on timestamp presence.
        assert!(result.unwrap().is_live);
    }

    #[test]
    fn test_vehicle_fallback_distance_based_eta() {
        let mut vehicles = HashMap::new();
        // Vehicle at (12.96, 77.58), no upcoming stop data.
        vehicles.insert(
            "bus_2".to_string(),
            make_vehicle(12.96, 77.58, Some(10.0), Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        let eta = result.unwrap();
        assert_eq!(eta.vehicle_id, "bus_2");
        assert!(eta.is_live);
        assert!(eta.eta_to_stop_seconds > 0);
    }

    #[test]
    fn test_vehicle_fallback_default_speed() {
        let mut vehicles = HashMap::new();
        // No speed reported but with a timestamp (live).
        vehicles.insert(
            "bus_3".to_string(),
            make_vehicle(12.96, 77.58, None, Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        let eta = result.unwrap();
        assert!(eta.is_live);
    }

    #[test]
    fn test_vehicle_without_timestamp_is_skipped() {
        let mut vehicles = HashMap::new();
        // No timestamp (not live) - should be skipped.
        vehicles.insert(
            "bus_no_ts".to_string(),
            make_vehicle(12.96, 77.58, None, None, None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_none());
    }

    #[test]
    fn test_stale_vehicle_is_skipped() {
        let mut vehicles = HashMap::new();
        // Timestamp is 10 minutes old (beyond 5-minute threshold).
        vehicles.insert(
            "bus_stale".to_string(),
            make_vehicle(
                12.96,
                77.58,
                Some(5.0),
                Some(Utc::now() - Duration::seconds(600)),
                None,
            ),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_none());
    }

    #[test]
    fn test_vehicle_slow_speed_uses_default() {
        let mut vehicles = HashMap::new();
        // Speed is 0.3 m/s (below 0.5 threshold), should use default 20 km/h.
        vehicles.insert(
            "bus_slow".to_string(),
            make_vehicle(12.96, 77.58, Some(0.3), Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        // With default speed, ETA should be distance / (20*1000/3600).
        let eta = result.unwrap();
        assert!(eta.eta_to_stop_seconds > 0);
    }

    #[test]
    fn test_multiple_vehicles_picks_closest() {
        let mut vehicles = HashMap::new();
        // Bus far away.
        vehicles.insert(
            "bus_far".to_string(),
            make_vehicle(13.00, 77.60, Some(5.0), Some(Utc::now()), None),
        );
        // Bus very close to the stop.
        vehicles.insert(
            "bus_close".to_string(),
            make_vehicle(12.9701, 77.5901, Some(5.0), Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        assert_eq!(result.unwrap().vehicle_id, "bus_close");
    }

    #[test]
    fn test_upcoming_stop_preferred_over_distance() {
        let mut vehicles = HashMap::new();
        // Bus with upcoming stop info giving 200s ETA.
        let upcoming = vec![make_upcoming_stop(
            "STOP_X",
            200,
            UpcomingStopStatus::Upcoming,
        )];
        vehicles.insert(
            "bus_with_stops".to_string(),
            make_vehicle(13.00, 77.60, Some(5.0), Some(Utc::now()), Some(upcoming)),
        );
        // Bus very close but with no upcoming stop info.
        vehicles.insert(
            "bus_close_no_stops".to_string(),
            make_vehicle(12.9701, 77.5901, Some(5.0), Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        let eta = result.unwrap();
        // The bus with upcoming stop data should be picked because its is_live flag
        // from upcoming stops prevents the second bus from overriding it (the code
        // only falls back when best is None or best.3 is false).
        assert_eq!(eta.vehicle_id, "bus_with_stops");
    }

    // ── Walking ETA calculation ─────────────────────────────────────────

    #[test]
    fn test_walking_speed_constant() {
        // 5 km/h = 1.388... m/s
        let expected = 5.0 * 1000.0 / 3600.0;
        assert!((WALKING_SPEED_MPS - expected).abs() < 0.001);
    }

    #[test]
    fn test_walking_buffer_constant() {
        assert_eq!(WALKING_BUFFER_SECONDS, 60);
    }

    // ── distance_between_in_meters sanity ───────────────────────────────

    #[test]
    fn test_distance_same_point_is_zero() {
        let p = make_point(12.97, 77.59);
        let d = distance_between_in_meters(&p, &p);
        assert!(d.abs() < 0.01);
    }

    #[test]
    fn test_distance_known_points() {
        // Approximately 1.1 km between these two points in Bangalore.
        let p1 = make_point(12.9716, 77.5946); // MG Road
        let p2 = make_point(12.9810, 77.5960); // ~1km north
        let d = distance_between_in_meters(&p1, &p2);
        assert!(d > 900.0 && d < 1500.0);
    }

    // ════════════════════════════════════════════════════════════════════
    // Edge case & robustness tests
    // ════════════════════════════════════════════════════════════════════

    // ── Zero distance to stop ──────────────────────────────────────────

    #[test]
    fn test_distance_zero_rider_at_stop() {
        // Rider is exactly at the stop location
        let p = make_point(12.97, 77.59);
        let d = distance_between_in_meters(&p, &p);
        assert!(d < 0.01, "Distance to self should be ~0");

        // Walking ETA for 0 distance should be 0
        let walking_eta = (d / WALKING_SPEED_MPS).ceil() as i64;
        assert_eq!(walking_eta, 0);
    }

    #[test]
    fn test_advisory_zero_walking_eta() {
        // Rider is at the stop (0 walking time). Vehicle arriving in 300s.
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 300,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        // walking + buffer = 0 + 60 = 60. Vehicle at 300. wait = 240 > 120 => can wait.
        let (should_leave, msg) = compute_advisory(0, &Some(eta));
        assert!(!should_leave);
        assert!(msg.contains("wait about 4 minutes"));
    }

    #[test]
    fn test_advisory_zero_walking_zero_vehicle_eta() {
        // Both walking and vehicle ETA are 0.
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 0,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        // walking + buffer = 60, vehicle at 0. 60 >= 0 => leave now.
        let (should_leave, _msg) = compute_advisory(0, &Some(eta));
        assert!(should_leave);
    }

    // ── Negative coordinates ───────────────────────────────────────────

    #[test]
    fn test_distance_negative_latitude() {
        // Southern hemisphere point (e.g., -33.86, 151.20 = Sydney)
        let p1 = make_point(-33.8688, 151.2093);
        let p2 = make_point(-33.8600, 151.2100);
        let d = distance_between_in_meters(&p1, &p2);
        assert!(
            d > 500.0 && d < 2000.0,
            "Distance should be reasonable for nearby Sydney points"
        );
    }

    #[test]
    fn test_distance_negative_longitude() {
        // Western hemisphere point (e.g., 40.71, -74.00 = NYC)
        let p1 = make_point(40.7128, -74.0060);
        let p2 = make_point(40.7200, -74.0060);
        let d = distance_between_in_meters(&p1, &p2);
        assert!(d > 500.0 && d < 1500.0);
    }

    #[test]
    fn test_distance_both_negative() {
        // Point in South America (-34.6, -58.4 = Buenos Aires)
        let p1 = make_point(-34.6037, -58.3816);
        let p2 = make_point(-34.6100, -58.3900);
        let d = distance_between_in_meters(&p1, &p2);
        assert!(d > 500.0 && d < 2000.0);
    }

    #[test]
    fn test_vehicle_at_negative_coordinates() {
        let mut vehicles = HashMap::new();
        vehicles.insert(
            "bus_south".to_string(),
            make_vehicle(-33.8688, 151.2093, Some(10.0), Some(Utc::now()), None),
        );

        let target = make_point(-33.8600, 151.2100);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        assert!(result.unwrap().eta_to_stop_seconds > 0);
    }

    // ── Very large radius queries / extreme distances ──────────────────

    #[test]
    fn test_distance_antipodal_points() {
        // Nearly antipodal points: maximum possible distance (~20,000 km)
        let p1 = make_point(0.0, 0.0);
        let p2 = make_point(0.0, 180.0);
        let d = distance_between_in_meters(&p1, &p2);
        // Half circumference of earth ~ 20,015 km
        assert!(d > 19_000_000.0 && d < 21_000_000.0);
    }

    #[test]
    fn test_vehicle_very_far_away() {
        // Vehicle is thousands of km from stop
        let mut vehicles = HashMap::new();
        vehicles.insert(
            "bus_far".to_string(),
            make_vehicle(0.0, 0.0, Some(10.0), Some(Utc::now()), None),
        );

        let target = make_point(40.0, 80.0);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        // ETA will be very large but should not panic or overflow
        let eta = result.unwrap();
        assert!(
            eta.eta_to_stop_seconds > 100_000,
            "Very distant vehicle should have very large ETA"
        );
    }

    // ── Points at exact boundary distances ─────────────────────────────

    #[test]
    fn test_distance_very_close_points() {
        // Two points about 1 meter apart
        let p1 = make_point(12.970000, 77.590000);
        let p2 = make_point(12.970009, 77.590000); // ~1m north
        let d = distance_between_in_meters(&p1, &p2);
        assert!(
            d > 0.5 && d < 3.0,
            "Should be approximately 1 meter apart, got {}",
            d
        );
    }

    #[test]
    fn test_advisory_boundary_walking_equals_vehicle_eta() {
        // Walking + buffer exactly equals vehicle ETA
        // walking_eta = 240, buffer = 60, total = 300. Vehicle ETA = 300.
        // 300 >= 300 => leave now
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 300,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, msg) = compute_advisory(240, &Some(eta));
        assert!(should_leave);
        assert!(msg.contains("Leave now to reach the stop before the bus arrives"));
    }

    #[test]
    fn test_advisory_boundary_one_second_over() {
        // walking_arrival (361) just exceeds vehicle ETA (360) by 1 second => leave now
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 360,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, _msg) = compute_advisory(300, &Some(eta));
        assert!(should_leave);
    }

    #[test]
    fn test_advisory_boundary_one_second_under() {
        // walking_arrival (360) < vehicle ETA (361). wait = 1 <= 120 => leave now (shortly after)
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 361,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        let (should_leave, msg) = compute_advisory(300, &Some(eta));
        assert!(should_leave);
        assert!(msg.contains("shortly after"));
    }

    // ── Empty transit data ─────────────────────────────────────────────

    #[test]
    fn test_find_best_vehicle_empty_map() {
        let vehicles: HashMap<String, VehicleTrackingInfo> = HashMap::new();
        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        let result = find_best_vehicle_eta(&vehicles, &target, "ANY_STOP", &scheduled);
        assert!(result.is_none(), "Empty vehicle map should return None");
    }

    #[test]
    fn test_vehicle_with_empty_upcoming_stops() {
        let mut vehicles = HashMap::new();
        vehicles.insert(
            "bus_empty_stops".to_string(),
            make_vehicle(12.96, 77.58, Some(5.0), Some(Utc::now()), Some(vec![])),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        // Empty upcoming stops list should fall back to distance-based
        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        assert!(result.unwrap().is_live);
    }

    #[test]
    fn test_vehicle_upcoming_stops_wrong_stop_code() {
        let mut vehicles = HashMap::new();
        let upcoming = vec![make_upcoming_stop(
            "STOP_Y",
            300,
            UpcomingStopStatus::Upcoming,
        )];
        vehicles.insert(
            "bus_wrong_stop".to_string(),
            make_vehicle(12.96, 77.58, Some(5.0), Some(Utc::now()), Some(upcoming)),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        // Upcoming stop code doesn't match target; falls back to distance-based
        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
    }

    // ── Vehicle speed edge cases ───────────────────────────────────────

    #[test]
    fn test_vehicle_zero_speed_uses_default() {
        let mut vehicles = HashMap::new();
        vehicles.insert(
            "bus_zero_speed".to_string(),
            make_vehicle(12.96, 77.58, Some(0.0), Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        // Speed 0.0 < 0.5 threshold, should use default 20 km/h
        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        assert!(result.unwrap().eta_to_stop_seconds > 0);
    }

    #[test]
    fn test_vehicle_very_fast_speed() {
        let mut vehicles = HashMap::new();
        // 100 m/s ~ 360 km/h
        vehicles.insert(
            "bus_fast".to_string(),
            make_vehicle(12.96, 77.58, Some(100.0), Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        let eta = result.unwrap();
        // At 100 m/s, ~1.5 km distance should take about 15 seconds
        assert!(eta.eta_to_stop_seconds < 100);
    }

    #[test]
    fn test_vehicle_speed_exactly_at_threshold() {
        let mut vehicles = HashMap::new();
        // Speed exactly at 0.5 m/s threshold should use 0.5 (not default)
        vehicles.insert(
            "bus_threshold".to_string(),
            make_vehicle(12.96, 77.58, Some(0.5), Some(Utc::now()), None),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(600));

        // 0.5 is NOT > 0.5, so default speed (20 km/h) should be used
        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
    }

    // ── Walking distance rounding ──────────────────────────────────────

    #[test]
    fn test_walking_distance_rounds_to_one_decimal() {
        // Verify the rounding formula: (distance * 10.0).round() / 10.0
        let test_cases: Vec<(f64, f64)> = vec![
            (1234.56, 1234.6),
            (0.0, 0.0),
            (999.94, 999.9),
            (999.95, 1000.0),
        ];
        for (input, expected) in test_cases {
            let rounded: f64 = (input * 10.0).round() / 10.0;
            assert!(
                (rounded - expected).abs() < 0.01,
                "Expected {} for input {}, got {}",
                expected,
                input,
                rounded
            );
        }
    }

    // ── Advisory message edge cases ────────────────────────────────────

    #[test]
    fn test_advisory_very_large_walking_eta() {
        // Walking ETA is very large (e.g., 10 hours)
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 100,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        // walking + buffer = 36060. Vehicle at 100. 36060 >= 100 => leave now.
        let (should_leave, msg) = compute_advisory(36000, &Some(eta));
        assert!(should_leave);
        assert!(msg.contains("Leave now to reach the stop before the bus arrives"));
    }

    #[test]
    fn test_advisory_very_large_vehicle_eta() {
        // Vehicle ETA is very large (e.g., 2 hours)
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 7200,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        // walking + buffer = 360. Vehicle at 7200. wait = 6840 > 120.
        // wait_minutes = 6840 / 60 = 114
        let (should_leave, msg) = compute_advisory(300, &Some(eta));
        assert!(!should_leave);
        assert!(msg.contains("wait about 114 minutes"));
    }

    #[test]
    fn test_advisory_negative_walking_eta() {
        // Negative walking ETA should not panic (edge case, shouldn't happen in practice)
        let eta = VehicleEta {
            vehicle_id: "v1".to_string(),
            eta_to_stop_seconds: 300,
            current_delay_seconds: 0,
            is_live: true,
            last_updated: TimeStamp(Utc::now()),
        };
        // walking + buffer = -100 + 60 = -40. -40 < 300 => wait.
        let (should_leave, _msg) = compute_advisory(-100, &Some(eta));
        assert!(
            !should_leave,
            "Negative walking ETA + buffer is below vehicle ETA, can wait"
        );
    }

    // ── Multiple upcoming stops for same vehicle ───────────────────────

    #[test]
    fn test_vehicle_multiple_upcoming_stops_picks_first_match() {
        let mut vehicles = HashMap::new();
        let upcoming = vec![
            make_upcoming_stop("STOP_Y", 100, UpcomingStopStatus::Upcoming),
            make_upcoming_stop("STOP_X", 200, UpcomingStopStatus::Upcoming),
            make_upcoming_stop("STOP_X", 500, UpcomingStopStatus::Upcoming), // duplicate stop
        ];
        vehicles.insert(
            "bus_multi".to_string(),
            make_vehicle(12.96, 77.58, Some(5.0), Some(Utc::now()), Some(upcoming)),
        );

        let target = make_point(12.97, 77.59);
        let scheduled = TimeStamp(Utc::now() + Duration::seconds(300));

        let result = find_best_vehicle_eta(&vehicles, &target, "STOP_X", &scheduled);
        assert!(result.is_some());
        let eta = result.unwrap();
        // Should pick the first STOP_X match (200s), not the second (500s)
        assert!((eta.eta_to_stop_seconds - 200).abs() <= 2);
    }

    // ── Input validation tests ──────────────────────────────────────────

    fn make_request(
        rider_lat: f64,
        rider_lon: f64,
        stop_lat: f64,
        stop_lon: f64,
        route_code: &str,
        stop_code: &str,
    ) -> TransitProximityRequest {
        TransitProximityRequest {
            rider_location: make_point(rider_lat, rider_lon),
            target_stop_location: make_point(stop_lat, stop_lon),
            route_code: route_code.to_string(),
            target_stop_code: stop_code.to_string(),
            scheduled_departure: TimeStamp(Utc::now() + Duration::seconds(600)),
            gtfs_id: "gtfs_1".to_string(),
        }
    }

    #[test]
    fn test_validate_request_valid() {
        let req = make_request(12.97, 77.59, 12.98, 77.60, "ROUTE_1", "STOP_X");
        assert!(validate_request(&req).is_ok());
    }

    #[test]
    fn test_validate_request_latitude_out_of_range() {
        let req = make_request(91.0, 77.59, 12.98, 77.60, "ROUTE_1", "STOP_X");
        let err = validate_request(&req);
        assert!(err.is_err());
    }

    #[test]
    fn test_validate_request_negative_latitude_out_of_range() {
        let req = make_request(-91.0, 77.59, 12.98, 77.60, "ROUTE_1", "STOP_X");
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_longitude_out_of_range() {
        let req = make_request(12.97, 181.0, 12.98, 77.60, "ROUTE_1", "STOP_X");
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_stop_latitude_out_of_range() {
        let req = make_request(12.97, 77.59, 95.0, 77.60, "ROUTE_1", "STOP_X");
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_empty_route_code() {
        let req = make_request(12.97, 77.59, 12.98, 77.60, "", "STOP_X");
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_whitespace_route_code() {
        let req = make_request(12.97, 77.59, 12.98, 77.60, "   ", "STOP_X");
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_empty_stop_code() {
        let req = make_request(12.97, 77.59, 12.98, 77.60, "ROUTE_1", "");
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_empty_gtfs_id() {
        let mut req = make_request(12.97, 77.59, 12.98, 77.60, "ROUTE_1", "STOP_X");
        req.gtfs_id = "".to_string();
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_whitespace_gtfs_id() {
        let mut req = make_request(12.97, 77.59, 12.98, 77.60, "ROUTE_1", "STOP_X");
        req.gtfs_id = "   ".to_string();
        assert!(validate_request(&req).is_err());
    }

    #[test]
    fn test_validate_request_boundary_latitude_90() {
        // Exactly 90 is valid (North Pole)
        let req = make_request(90.0, 0.0, -90.0, 0.0, "ROUTE_1", "STOP_X");
        assert!(validate_request(&req).is_ok());
    }

    #[test]
    fn test_validate_request_boundary_longitude_180() {
        // Exactly 180 is valid (International Date Line)
        let req = make_request(0.0, 180.0, 0.0, -180.0, "ROUTE_1", "STOP_X");
        assert!(validate_request(&req).is_ok());
    }

    // ── Stress / load test scenarios (documented, not executed) ────────
    //
    // 1. CONCURRENT TRANSIT PROXIMITY REQUESTS
    //    - Simulate 1000 concurrent POST /internal/transit/proximity requests
    //      with varied rider locations and route codes.
    //    - Expected: all return 200, p99 latency < 200ms.
    //    - Watch for: Redis connection pool exhaustion, haversine calculation
    //      precision under concurrent load.
    //
    // 2. LARGE VEHICLE MAP
    //    - Route with 500+ tracked vehicles, each with 20+ upcoming stops.
    //    - Expected: find_best_vehicle_eta completes in < 10ms.
    //    - Watch for: O(n*m) scan time, HashMap iteration overhead.
    //
    // 3. RAPID LOCATION UPDATES
    //    - Vehicle positions updating every 100ms while proximity queries run.
    //    - Expected: no stale data crashes, consistent advisory output.
    //    - Watch for: Redis race conditions, timestamp comparison edge cases.
}
