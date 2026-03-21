/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use std::collections::VecDeque;

use crate::common::{
    types::{
        DetectionContext, Latitude, Longitude, Point, SafetyCheckState, StopDetectionState,
        ViolationDetectionState,
    },
    utils::distance_between_in_meters,
};

fn create_stop_detection_state(
    total_datapoints: u64,
    avg_speed: Option<VecDeque<(f64, u32)>>,
    avg_coord_mean: VecDeque<(Point, u32)>,
) -> ViolationDetectionState {
    ViolationDetectionState::StopDetection(StopDetectionState {
        total_datapoints,
        avg_speed,
        avg_coord_mean,
    })
}

fn create_safety_check_state(
    total_datapoints: u64,
    avg_speed: Option<VecDeque<(f64, u32)>>,
    avg_coord_mean: VecDeque<(Point, u32)>,
) -> ViolationDetectionState {
    ViolationDetectionState::SafetyCheck(SafetyCheckState {
        total_datapoints,
        avg_speed,
        avg_coord_mean,
    })
}

pub fn handle_stop_or_safety_check(
    state: Option<ViolationDetectionState>,
    context: &DetectionContext,
    sample_size: u32,
    batch_count: u32,
    max_eligible_distance: u32,
    is_anti_violation: bool,
    is_safety_check: bool,
) -> Option<(ViolationDetectionState, Option<bool>)> {
    let create_state = if is_safety_check {
        create_safety_check_state
    } else {
        create_stop_detection_state
    };

    if let Some(
        ViolationDetectionState::StopDetection(StopDetectionState {
            avg_speed,
            mut total_datapoints,
            mut avg_coord_mean,
        })
        | ViolationDetectionState::SafetyCheck(SafetyCheckState {
            avg_speed,
            mut total_datapoints,
            mut avg_coord_mean,
        }),
    ) = state
    {
        if let Some(prev_tuple) = avg_coord_mean.back() {
            let max_batch_size = sample_size.div_ceil(batch_count);
            let prev_avg_point = prev_tuple.0;
            let prev_batch_datapoints = prev_tuple.1;
            if total_datapoints < sample_size as u64 {
                if prev_batch_datapoints < max_batch_size {
                    let current_avg_point = Point {
                        lat: Latitude(
                            (prev_avg_point.lat.inner() * prev_batch_datapoints as f64
                                + context.location.lat.inner())
                                / (prev_batch_datapoints as f64 + 1.0),
                        ),
                        lon: Longitude(
                            (prev_avg_point.lon.inner() * prev_batch_datapoints as f64
                                + context.location.lon.inner())
                                / (prev_batch_datapoints as f64 + 1.0),
                        ),
                    };
                    avg_coord_mean.pop_back();
                    avg_coord_mean.push_back((current_avg_point, prev_batch_datapoints + 1));
                    total_datapoints += 1;
                    if total_datapoints == sample_size as u64 {
                        if let Some(true) = check_stopped(&avg_coord_mean, max_eligible_distance) {
                            return Some((
                                create_state(total_datapoints, avg_speed, avg_coord_mean),
                                Some(!is_anti_violation),
                            ));
                        } else if let Some(false) =
                            check_stopped(&avg_coord_mean, max_eligible_distance)
                        {
                            return Some((
                                create_state(total_datapoints, avg_speed, avg_coord_mean),
                                Some(is_anti_violation),
                            ));
                        }
                    }
                    return Some((
                        create_state(total_datapoints, avg_speed, avg_coord_mean),
                        None,
                    ));
                } else {
                    avg_coord_mean.push_back((context.location, 1));
                    total_datapoints += 1;
                    return Some((
                        create_state(total_datapoints, avg_speed, avg_coord_mean),
                        None,
                    ));
                }
            } else {
                total_datapoints -= max_batch_size as u64;
                let mut without_front = avg_coord_mean.clone();
                without_front.pop_front();
                if let Some(prev_tuple) = without_front.back() {
                    let prev_average_point = prev_tuple.0;
                    let prev_batch_size = prev_tuple.1;
                    if prev_batch_size < max_batch_size {
                        let current_avg_point = Point {
                            lat: Latitude(
                                (prev_average_point.lat.inner() * prev_batch_size as f64
                                    + context.location.lat.inner())
                                    / (prev_batch_size as f64 + 1.0),
                            ),
                            lon: Longitude(
                                (prev_average_point.lon.inner() * prev_batch_size as f64
                                    + context.location.lon.inner())
                                    / (prev_batch_size as f64 + 1.0),
                            ),
                        };
                        avg_coord_mean.pop_back();
                        avg_coord_mean.push_back((current_avg_point, prev_batch_size + 1));
                        total_datapoints += 1;
                        if total_datapoints == sample_size as u64 {
                            if let Some(true) =
                                check_stopped(&avg_coord_mean, max_eligible_distance)
                            {
                                return Some((
                                    create_state(total_datapoints, avg_speed, avg_coord_mean),
                                    Some(!is_anti_violation),
                                ));
                            } else if let Some(false) =
                                check_stopped(&avg_coord_mean, max_eligible_distance)
                            {
                                return Some((
                                    create_state(total_datapoints, avg_speed, avg_coord_mean),
                                    Some(is_anti_violation),
                                ));
                            }
                        }
                        return Some((
                            create_state(total_datapoints, avg_speed, avg_coord_mean),
                            None,
                        ));
                    } else {
                        without_front.push_back((context.location, 1));
                        total_datapoints += 1;
                        return Some((
                            create_state(total_datapoints, avg_speed, without_front),
                            None,
                        ));
                    }
                } else {
                    // Handle case when without_front.back() returns None after pop_front()
                    total_datapoints = 1;
                    return Some((
                        create_state(
                            total_datapoints,
                            avg_speed,
                            VecDeque::from([(context.location, 1)]),
                        ),
                        None,
                    ));
                }
            }
        } else {
            // Handle case when avg_coord_mean.back() returns None (empty list)
            total_datapoints = 1;
            return Some((
                create_state(
                    total_datapoints,
                    avg_speed,
                    VecDeque::from([(context.location, 1)]),
                ),
                None,
            ));
        }
    }
    Some((
        create_state(1, None, VecDeque::from([(context.location, 1)])),
        None,
    ))
}

/// Checks if a vehicle has stopped by comparing the distance between the first and last point
/// in the provided list of coordinates. If the distance is less than max_eligible_distance,
/// the vehicle is considered to have stopped.
fn check_stopped(coord_list: &VecDeque<(Point, u32)>, max_eligible_distance: u32) -> Option<bool> {
    if let (Some(first_point), Some(last_point)) = (coord_list.front(), coord_list.back()) {
        let distance = distance_between_in_meters(&first_point.0, &last_point.0);
        Some(distance <= max_eligible_distance as f64)
    } else {
        None
    }
}
