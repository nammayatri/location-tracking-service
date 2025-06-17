/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use std::collections::VecDeque;

use tracing::warn;

use crate::{
    common::flow::safety_check::handle_stop_or_safety_check,
    common::types::*,
    common::utils::{distance_between_in_meters, get_upcoming_stops_by_route_code},
    outbound::types::{
        DetectionData, OppositeDirectionDetectionData, OverSpeedingDetectionData,
        RouteDeviationDetectionData, SafetyCheckDetectionData, StoppedDetectionData,
        ViolationDetectionReq,
    },
};

use super::utils::find_closest_point_on_route;
use crate::outbound::types::RideStopReachedDetectionData;

pub fn check(
    violation_config: &ViolationDetectionConfig,
    anti_violation_config: &ViolationDetectionConfig,
    context: DetectionContext,
    detection_state: Option<ViolationDetectionState>,
    anti_detection_state: Option<ViolationDetectionState>,
    violation_detection_trigger_flag: Option<DetectionStatus>,
) -> (
    Option<ViolationDetectionState>,
    Option<ViolationDetectionState>,
    Option<DetectionStatus>,
    Option<ViolationDetectionReq>,
) {
    let violation_state_and_trigger =
        violation_check(violation_config, &context, detection_state.to_owned());
    let anti_violation_state_and_trigger = anti_violation_check(
        anti_violation_config,
        &context,
        anti_detection_state.to_owned(),
    );

    let violation_state = violation_state_and_trigger.as_ref().map(|(state, _)| state);
    let anti_violation_state = anti_violation_state_and_trigger
        .as_ref()
        .map(|(state, _)| state);

    let is_violated = violation_trigger_decision_tree(
        violation_detection_trigger_flag.as_ref(),
        violation_state_and_trigger
            .as_ref()
            .and_then(|(_, trigger)| trigger.to_owned()),
        anti_violation_state_and_trigger
            .as_ref()
            .and_then(|(_, trigger)| trigger.to_owned()),
    );

    let trigger_detection_req = get_triggered_state_req(
        violation_state,
        anti_violation_state,
        is_violated.clone(),
        context,
    );

    (
        violation_state.cloned(),
        anti_violation_state.cloned(),
        is_violated,
        trigger_detection_req,
    )
}

fn violation_check(
    config: &ViolationDetectionConfig,
    context: &DetectionContext,
    state: Option<ViolationDetectionState>,
) -> Option<(ViolationDetectionState, Option<bool>)> {
    match config.detection_config {
        DetectionConfig::OverspeedingDetection(OverspeedingConfig {
            sample_size,
            speed_limit,
            batch_count,
        }) => {
            if let Some(ViolationDetectionState::Overspeeding(OverspeedingState {
                mut total_datapoints,
                avg_speed_record,
            })) = state
            {
                if let (Some(prev_tuple), Some(current_speed)) =
                    (avg_speed_record.back(), context.speed)
                {
                    let max_batch_size = sample_size.div_ceil(batch_count);
                    let prev_avg_speed = prev_tuple.0;
                    let prev_batch_datapoints = prev_tuple.1;
                    if total_datapoints < sample_size as u64 {
                        if prev_batch_datapoints < max_batch_size {
                            let current_avg_speed = (prev_avg_speed * prev_batch_datapoints as f64
                                + current_speed.inner())
                                / (prev_batch_datapoints as f64 + 1.0);
                            let mut copy_list = avg_speed_record.clone();
                            copy_list.pop_back();
                            copy_list.push_back((current_avg_speed, prev_batch_datapoints + 1));
                            total_datapoints += 1;
                            if total_datapoints == sample_size as u64 {
                                // get this by the average of the values fro the list in the state
                                let current_average_speed =
                                    copy_list.iter().map(|c| c.0).sum::<f64>()
                                        / (copy_list.len() as f64);
                                if current_average_speed > speed_limit {
                                    return Some((
                                        ViolationDetectionState::Overspeeding(OverspeedingState {
                                            total_datapoints,
                                            avg_speed_record: copy_list,
                                        }),
                                        Some(true),
                                    ));
                                } else {
                                    return Some((
                                        ViolationDetectionState::Overspeeding(OverspeedingState {
                                            total_datapoints,
                                            avg_speed_record: copy_list,
                                        }),
                                        Some(false),
                                    ));
                                }
                            }
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    total_datapoints,
                                    avg_speed_record: copy_list,
                                }),
                                None,
                            ));
                        } else {
                            let mut copy_list = avg_speed_record.clone();
                            copy_list.push_back((current_speed.inner(), 1));
                            total_datapoints += 1;
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    total_datapoints,
                                    avg_speed_record: copy_list,
                                }),
                                None,
                            ));
                        }
                    } else {
                        let mut copy_list = avg_speed_record;
                        total_datapoints -= max_batch_size as u64;
                        copy_list.pop_front();
                        if let Some(prev_tuple) = copy_list.back() {
                            let prev_average_speed = prev_tuple.0;
                            let prev_batch_size = prev_tuple.1;
                            if prev_batch_size < max_batch_size {
                                let current_avg_speed = (prev_average_speed
                                    * prev_batch_size as f64
                                    + current_speed.inner())
                                    / (prev_batch_size as f64 + 1.0);
                                copy_list.pop_back();
                                copy_list.push_back((current_avg_speed, prev_batch_datapoints + 1));
                                total_datapoints += 1;
                                if total_datapoints == sample_size as u64 {
                                    // get this by the average of the values fro the list in the state
                                    let current_average_speed =
                                        copy_list.iter().map(|c| c.0).sum::<f64>()
                                            / (copy_list.len() as f64);
                                    if current_average_speed > speed_limit {
                                        return Some((
                                            ViolationDetectionState::Overspeeding(
                                                OverspeedingState {
                                                    total_datapoints,
                                                    avg_speed_record: copy_list,
                                                },
                                            ),
                                            Some(true),
                                        ));
                                    } else {
                                        return Some((
                                            ViolationDetectionState::Overspeeding(
                                                OverspeedingState {
                                                    total_datapoints,
                                                    avg_speed_record: copy_list,
                                                },
                                            ),
                                            Some(false),
                                        ));
                                    }
                                }
                                return Some((
                                    ViolationDetectionState::Overspeeding(OverspeedingState {
                                        total_datapoints,
                                        avg_speed_record: copy_list,
                                    }),
                                    None,
                                ));
                            } else {
                                copy_list.push_back((current_speed.inner(), 1));
                                total_datapoints += 1;
                                return Some((
                                    ViolationDetectionState::Overspeeding(OverspeedingState {
                                        total_datapoints,
                                        avg_speed_record: copy_list,
                                    }),
                                    None,
                                ));
                            }
                        } else {
                            total_datapoints = 1;
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    total_datapoints,
                                    avg_speed_record: VecDeque::from([(current_speed.inner(), 1)]),
                                }),
                                None,
                            ));
                        }
                    }
                }
            } else if let Some(current_speed) = context.speed {
                return Some((
                    ViolationDetectionState::Overspeeding(OverspeedingState {
                        total_datapoints: 1,
                        avg_speed_record: VecDeque::from([(current_speed.inner(), 1)]),
                    }),
                    None,
                ));
            }
            None
        }
        DetectionConfig::RouteDeviationDetection(RouteDeviationConfig {
            batch_count,
            deviation_threshold,
            sample_size,
        }) => {
            if let Some(ViolationDetectionState::RouteDeviation(RouteDeviationState {
                deviation_distance, // update this value down in the code below
                mut total_datapoints,
                avg_deviation_record,
            })) = state
            {
                if let Some(prev_tuple) = avg_deviation_record.back() {
                    let max_batch_size = sample_size.div_ceil(batch_count);
                    let prev_avg_point = prev_tuple.0.clone();
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
                            let mut copy_list = avg_deviation_record.clone();
                            copy_list.pop_back();
                            copy_list.push_back((current_avg_point, prev_batch_datapoints + 1));
                            total_datapoints += 1;
                            if total_datapoints == sample_size as u64 {
                                if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                                    if let Some((true, distance)) = check_deviated(
                                        &copy_list,
                                        deviation_threshold,
                                        context.route,
                                    ) {
                                        return Some((
                                            ViolationDetectionState::RouteDeviation(
                                                RouteDeviationState {
                                                    total_datapoints,
                                                    avg_deviation_record: copy_list,
                                                    deviation_distance: distance,
                                                },
                                            ),
                                            Some(true),
                                        ));
                                    } else if let Some((false, distance)) = check_deviated(
                                        &copy_list,
                                        deviation_threshold,
                                        context.route,
                                    ) {
                                        return Some((
                                            ViolationDetectionState::RouteDeviation(
                                                RouteDeviationState {
                                                    total_datapoints,
                                                    avg_deviation_record: copy_list,
                                                    deviation_distance: distance,
                                                },
                                            ),
                                            Some(false),
                                        ));
                                    }
                                }
                            }
                            return Some((
                                ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                    total_datapoints,
                                    avg_deviation_record: copy_list,
                                    deviation_distance,
                                }),
                                None,
                            ));
                        } else {
                            let mut copy_list = avg_deviation_record.clone();
                            copy_list.push_back((context.location.clone(), 1));
                            total_datapoints += 1;
                            return Some((
                                ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                    total_datapoints,
                                    avg_deviation_record: copy_list,
                                    deviation_distance,
                                }),
                                None,
                            ));
                        }
                    } else {
                        let mut copy_list = avg_deviation_record.clone();
                        total_datapoints -= max_batch_size as u64;
                        copy_list.pop_front();
                        if let Some(prev_tuple) = copy_list.back() {
                            let prev_average_point = prev_tuple.0.clone();
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
                                let mut copy_list = avg_deviation_record.clone();
                                copy_list.pop_back();
                                copy_list.push_back((current_avg_point, prev_batch_size + 1));
                                total_datapoints += 1;
                                if total_datapoints == sample_size as u64 {
                                    if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                                        if let Some((true, distance)) = check_deviated(
                                            &copy_list,
                                            deviation_threshold,
                                            context.route,
                                        ) {
                                            return Some((
                                                ViolationDetectionState::RouteDeviation(
                                                    RouteDeviationState {
                                                        total_datapoints,
                                                        avg_deviation_record: copy_list,
                                                        deviation_distance: distance,
                                                    },
                                                ),
                                                Some(true),
                                            ));
                                        } else if let Some((false, distance)) = check_deviated(
                                            &copy_list,
                                            deviation_threshold,
                                            context.route,
                                        ) {
                                            return Some((
                                                ViolationDetectionState::RouteDeviation(
                                                    RouteDeviationState {
                                                        total_datapoints,
                                                        avg_deviation_record: copy_list,
                                                        deviation_distance: distance,
                                                    },
                                                ),
                                                Some(false),
                                            ));
                                        }
                                    }
                                }
                                return Some((
                                    ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                        total_datapoints,
                                        avg_deviation_record: copy_list,
                                        deviation_distance,
                                    }),
                                    None,
                                ));
                            } else {
                                copy_list.push_back((context.location.clone(), 1));
                                total_datapoints += 1;
                                return Some((
                                    ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                        total_datapoints,
                                        avg_deviation_record: copy_list,
                                        deviation_distance,
                                    }),
                                    None,
                                ));
                            }
                        }
                    }
                }
            }
            Some((
                ViolationDetectionState::RouteDeviation(RouteDeviationState {
                    total_datapoints: 1,
                    avg_deviation_record: VecDeque::from([(context.location.clone(), 1)]),
                    deviation_distance: 0.0,
                }),
                None,
            ))
        }
        DetectionConfig::StoppedDetection(StoppedDetectionConfig {
            batch_count,
            max_eligible_speed: _,
            max_eligible_distance,
            sample_size,
        }) => handle_stop_or_safety_check(
            state,
            context,
            sample_size,
            batch_count,
            max_eligible_distance,
            false,
            false,
        ),
        DetectionConfig::OppositeDirectionDetection(OppositeDirectionConfig {}) => {
            if let Some(ViolationDetectionState::OppositeDirection(OppositeDirectionState {
                total_datapoints,
                expected_count,
            })) = state
            {
                if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                    if let Some(route) = context.route {
                        let upcoming_stops =
                            get_upcoming_stops_by_route_code(None, route, &context.location);
                        if let Ok(upcoming) = upcoming_stops {
                            let count = upcoming.len() as u64;
                            if count <= expected_count {
                                return Some((
                                    ViolationDetectionState::OppositeDirection(
                                        OppositeDirectionState {
                                            total_datapoints: total_datapoints + 1,
                                            expected_count: count,
                                        },
                                    ),
                                    Some(false),
                                ));
                            } else {
                                return Some((
                                    ViolationDetectionState::OppositeDirection(
                                        OppositeDirectionState {
                                            total_datapoints: total_datapoints + 1,
                                            expected_count,
                                        },
                                    ),
                                    Some(true),
                                ));
                            }
                        }
                        return Some((
                            ViolationDetectionState::OppositeDirection(OppositeDirectionState {
                                total_datapoints: total_datapoints + 1,
                                expected_count,
                            }),
                            None,
                        ));
                    }
                }
            }
            Some((
                ViolationDetectionState::OppositeDirection(OppositeDirectionState {
                    total_datapoints: 0,
                    expected_count: 1e6 as u64,
                }),
                None,
            ))
        }
        DetectionConfig::TripNotStartedDetection(TripNotStartedConfig {
            deviation_threshold,
            sample_size,
            batch_count,
        }) => {
            if let Some(ViolationDetectionState::TripNotStarted(TripNotStartedState {
                mut total_datapoints,
                avg_coord_mean,
            })) = state
            {
                let max_batch_size = sample_size.div_ceil(batch_count);
                let flag = context.ride_status == RideStatus::NEW;

                if let Some(prev_tuple) = avg_coord_mean.back() {
                    let prev_avg_point = prev_tuple.0.clone();
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
                            let mut copy_list = avg_coord_mean.clone();
                            copy_list.pop_back();
                            copy_list.push_back((current_avg_point, prev_batch_datapoints + 1));
                            total_datapoints += 1;

                            if total_datapoints == sample_size as u64 && flag {
                                if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                                    if let Some(true) = check_trip_not_started(
                                        &copy_list,
                                        deviation_threshold,
                                        context.route,
                                    ) {
                                        return Some((
                                            ViolationDetectionState::TripNotStarted(
                                                TripNotStartedState {
                                                    total_datapoints,
                                                    avg_coord_mean: copy_list,
                                                },
                                            ),
                                            Some(true),
                                        ));
                                    } else if let Some(false) = check_trip_not_started(
                                        &copy_list,
                                        deviation_threshold,
                                        context.route,
                                    ) {
                                        return Some((
                                            ViolationDetectionState::TripNotStarted(
                                                TripNotStartedState {
                                                    total_datapoints,
                                                    avg_coord_mean: copy_list,
                                                },
                                            ),
                                            Some(false),
                                        ));
                                    }
                                }
                            }
                            return Some((
                                ViolationDetectionState::TripNotStarted(TripNotStartedState {
                                    total_datapoints,
                                    avg_coord_mean: copy_list,
                                }),
                                None,
                            ));
                        } else {
                            let mut copy_list = avg_coord_mean.clone();
                            copy_list.push_back((context.location.clone(), 1));
                            total_datapoints += 1;
                            return Some((
                                ViolationDetectionState::TripNotStarted(TripNotStartedState {
                                    total_datapoints,
                                    avg_coord_mean: copy_list,
                                }),
                                None,
                            ));
                        }
                    } else {
                        let mut copy_list = avg_coord_mean.clone();
                        total_datapoints -= max_batch_size as u64;
                        copy_list.pop_front();
                        if let Some(prev_tuple) = copy_list.back() {
                            let prev_average_point = prev_tuple.0.clone();
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
                                let mut copy_list = avg_coord_mean.clone();
                                copy_list.pop_back();
                                copy_list.push_back((current_avg_point, prev_batch_size + 1));
                                total_datapoints += 1;

                                if total_datapoints == sample_size as u64 && flag {
                                    if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                                        if let Some(true) = check_trip_not_started(
                                            &copy_list,
                                            deviation_threshold,
                                            context.route,
                                        ) {
                                            return Some((
                                                ViolationDetectionState::TripNotStarted(
                                                    TripNotStartedState {
                                                        total_datapoints,
                                                        avg_coord_mean: copy_list,
                                                    },
                                                ),
                                                Some(true),
                                            ));
                                        } else if let Some(false) = check_trip_not_started(
                                            &copy_list,
                                            deviation_threshold,
                                            context.route,
                                        ) {
                                            return Some((
                                                ViolationDetectionState::TripNotStarted(
                                                    TripNotStartedState {
                                                        total_datapoints,
                                                        avg_coord_mean: copy_list,
                                                    },
                                                ),
                                                Some(false),
                                            ));
                                        }
                                    }
                                }
                                return Some((
                                    ViolationDetectionState::TripNotStarted(TripNotStartedState {
                                        total_datapoints,
                                        avg_coord_mean: copy_list,
                                    }),
                                    None,
                                ));
                            }
                        }
                    }
                }
            }
            Some((
                ViolationDetectionState::TripNotStarted(TripNotStartedState {
                    total_datapoints: 1,
                    avg_coord_mean: VecDeque::from([(context.location.clone(), 1)]),
                }),
                None,
            ))
        }
        DetectionConfig::SafetyCheckDetection(SafetyCheckConfig {
            batch_count,
            sample_size,
            max_eligible_speed: _,
            max_eligible_distance,
        }) => handle_stop_or_safety_check(
            state,
            context,
            sample_size,
            batch_count,
            max_eligible_distance,
            false,
            true,
        ),
        DetectionConfig::RideStopReachedDetection(RideStopReachedConfig {
            sample_size,
            batch_count,
            stop_reach_threshold,
            min_stop_duration,
        }) => handle_ride_stop_reached_check(
            state,
            context,
            sample_size,
            batch_count,
            stop_reach_threshold,
            min_stop_duration,
            false,
        ),
    }
}
/*
Detects when a driver has reached a specific ride stop.
This function works by:
1. Collecting location data in batches over a sample_size period
2. Calculating distance to the next stop in the route
3. Triggering detection when driver is within stop_reach_threshold meters of the stop
*/
fn handle_ride_stop_reached_check(
    state: Option<ViolationDetectionState>,
    context: &DetectionContext,
    sample_size: u32,
    batch_count: u32,
    stop_reach_threshold: u32,
    min_stop_duration: u32,
    is_anti_violation: bool,
) -> Option<(ViolationDetectionState, Option<bool>)> {
    let ride_stops = context.ride_stops?;

    let _stop_duration = min_stop_duration;

    if ride_stops.is_empty() {
        return None;
    }

    let (current_stop_index, reached_stops, mut total_datapoints, mut avg_coord_mean) =
        if let Some(ViolationDetectionState::RideStopReached(RideStopReachedState {
            current_stop_index,
            reached_stops,
            total_datapoints,
            avg_coord_mean,
        })) = state
        {
            (
                current_stop_index,
                reached_stops,
                total_datapoints,
                avg_coord_mean,
            )
        } else {
            (0, vec![], 0, VecDeque::new())
        };

    if current_stop_index >= ride_stops.len() {
        return None;
    }

    let next_stop = &ride_stops[current_stop_index];
    let distance_to_stop = distance_between_in_meters(&context.location, next_stop);

    total_datapoints += 1;

    if let Some(prev_tuple) = avg_coord_mean.back() {
        let max_batch_size = sample_size.div_ceil(batch_count);
        let prev_avg_point = prev_tuple.0.clone();
        let prev_batch_datapoints = prev_tuple.1;

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
            let mut copy_list = avg_coord_mean.clone();
            copy_list.pop_back();
            copy_list.push_back((current_avg_point, prev_batch_datapoints + 1));
            avg_coord_mean = copy_list;
        } else {
            avg_coord_mean.push_back((context.location.clone(), 1));
        }
    } else {
        avg_coord_mean.push_back((context.location.clone(), 1));
    }

    if total_datapoints >= sample_size as u64 {
        let is_at_stop = distance_to_stop <= stop_reach_threshold as f64;

        let should_trigger = if is_anti_violation {
            !is_at_stop
        } else {
            is_at_stop
        };

        if should_trigger {
            if !is_anti_violation && is_at_stop {
                let mut new_reached_stops = reached_stops.clone();
                new_reached_stops.push(format!("stop_{}", current_stop_index));

                return Some((
                    ViolationDetectionState::RideStopReached(RideStopReachedState {
                        total_datapoints: 0,
                        reached_stops: new_reached_stops,
                        current_stop_index: current_stop_index + 1,
                        avg_coord_mean: VecDeque::new(),
                    }),
                    Some(true),
                ));
            } else {
                return Some((
                    ViolationDetectionState::RideStopReached(RideStopReachedState {
                        total_datapoints: 0,
                        reached_stops,
                        current_stop_index,
                        avg_coord_mean: VecDeque::new(),
                    }),
                    Some(false),
                ));
            }
        } else {
            return Some((
                ViolationDetectionState::RideStopReached(RideStopReachedState {
                    total_datapoints: 0,
                    reached_stops,
                    current_stop_index,
                    avg_coord_mean: VecDeque::new(),
                }),
                Some(false),
            ));
        }
    }

    Some((
        ViolationDetectionState::RideStopReached(RideStopReachedState {
            total_datapoints,
            reached_stops,
            current_stop_index,
            avg_coord_mean,
        }),
        None,
    ))
}

/// Checks if a vehicle has deviated from the route by using the average of the points in the list
/// first we get the route from the route_code in the context, and then we get the polyline for the route
/// and then we project the current point on the polyline and check if the distance between the projected point and the current point is less than the deviation threshold
fn check_deviated(
    coord_list: &VecDeque<(Point, u32)>,
    deviation_threshold: u32,
    points: Option<&Route>,
) -> Option<(bool, f64)> {
    if let Some(route) = points {
        let points = route
            .waypoints
            .iter()
            .map(|w| w.coordinate.clone())
            .collect();
        let average_point = {
            let mut total_lat = 0.0;
            let mut total_lon = 0.0;
            let count = coord_list.len() as f64;

            for (point, _) in coord_list.iter() {
                total_lat += point.lat.inner();
                total_lon += point.lon.inner();
            }

            Point {
                lat: Latitude(total_lat / count),
                lon: Longitude(total_lon / count),
            }
        };
        let projected_point = find_closest_point_on_route(&average_point, points);
        if let Some(projected_point) = projected_point {
            let distance =
                distance_between_in_meters(&average_point, &projected_point.projection_point);
            return Some((distance > deviation_threshold as f64, distance));
        }
    }
    None
}

fn anti_violation_check(
    config: &ViolationDetectionConfig,
    context: &DetectionContext,
    state: Option<ViolationDetectionState>,
) -> Option<(ViolationDetectionState, Option<bool>)> {
    match config.detection_config {
        DetectionConfig::OverspeedingDetection(OverspeedingConfig {
            sample_size,
            speed_limit,
            batch_count,
        }) => {
            if let Some(ViolationDetectionState::Overspeeding(OverspeedingState {
                mut total_datapoints,
                avg_speed_record,
            })) = state
            {
                if let (Some(prev_tuple), Some(current_speed)) =
                    (avg_speed_record.back(), context.speed)
                {
                    let max_batch_size = sample_size.div_ceil(batch_count);
                    let prev_avg_speed = prev_tuple.0;
                    let prev_batch_datapoints = prev_tuple.1;
                    if total_datapoints < sample_size as u64 {
                        if prev_batch_datapoints < max_batch_size {
                            let current_avg_speed = (prev_avg_speed * prev_batch_datapoints as f64
                                + current_speed.inner())
                                / (prev_batch_datapoints as f64 + 1.0);
                            let mut copy_list = avg_speed_record.clone();
                            copy_list.pop_back();
                            copy_list.push_back((current_avg_speed, prev_batch_datapoints + 1));
                            total_datapoints += 1;
                            if total_datapoints == sample_size as u64 {
                                // get this by the average of the values fro the list in the state
                                let current_average_speed =
                                    copy_list.iter().map(|c| c.0).sum::<f64>()
                                        / (copy_list.len() as f64);
                                if current_average_speed <= speed_limit {
                                    return Some((
                                        ViolationDetectionState::Overspeeding(OverspeedingState {
                                            total_datapoints,
                                            avg_speed_record: copy_list,
                                        }),
                                        Some(true),
                                    ));
                                } else {
                                    return Some((
                                        ViolationDetectionState::Overspeeding(OverspeedingState {
                                            total_datapoints,
                                            avg_speed_record: copy_list,
                                        }),
                                        Some(false),
                                    ));
                                }
                            }
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    total_datapoints,
                                    avg_speed_record: copy_list,
                                }),
                                None,
                            ));
                        } else {
                            let mut copy_list = avg_speed_record.clone();
                            copy_list.push_back((current_speed.inner(), 1));
                            total_datapoints += 1;
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    total_datapoints,
                                    avg_speed_record: copy_list,
                                }),
                                None,
                            ));
                        }
                    } else {
                        let mut copy_list = avg_speed_record;
                        total_datapoints -= max_batch_size as u64;
                        copy_list.pop_front();
                        if let Some(prev_tuple) = copy_list.back() {
                            let prev_average_speed = prev_tuple.0;
                            let prev_batch_size = prev_tuple.1;
                            // Starting opied
                            if prev_batch_size < max_batch_size {
                                let current_avg_speed = (prev_average_speed
                                    * prev_batch_size as f64
                                    + current_speed.inner())
                                    / (prev_batch_size as f64 + 1.0);
                                copy_list.pop_back();
                                copy_list.push_back((current_avg_speed, prev_batch_datapoints + 1));
                                total_datapoints += 1;
                                if total_datapoints == sample_size as u64 {
                                    // get this by the average of the values fro the list in the state
                                    let current_average_speed =
                                        copy_list.iter().map(|c| c.0).sum::<f64>()
                                            / (copy_list.len() as f64);
                                    if current_average_speed <= speed_limit {
                                        return Some((
                                            ViolationDetectionState::Overspeeding(
                                                OverspeedingState {
                                                    total_datapoints,
                                                    avg_speed_record: copy_list,
                                                },
                                            ),
                                            Some(true),
                                        ));
                                    } else {
                                        return Some((
                                            ViolationDetectionState::Overspeeding(
                                                OverspeedingState {
                                                    total_datapoints,
                                                    avg_speed_record: copy_list,
                                                },
                                            ),
                                            Some(false),
                                        ));
                                    }
                                }
                                return Some((
                                    ViolationDetectionState::Overspeeding(OverspeedingState {
                                        total_datapoints,
                                        avg_speed_record: copy_list,
                                    }),
                                    None,
                                ));
                            } else {
                                copy_list.push_back((current_speed.inner(), 1));
                                total_datapoints += 1;
                                return Some((
                                    ViolationDetectionState::Overspeeding(OverspeedingState {
                                        total_datapoints,
                                        avg_speed_record: copy_list,
                                    }),
                                    None,
                                ));
                            }
                            // Ending copied
                        } else {
                            total_datapoints = 1;
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    total_datapoints,
                                    avg_speed_record: VecDeque::from([(current_speed.inner(), 1)]),
                                }),
                                None,
                            ));
                        }
                    }
                }
            } else if let Some(current_speed) = context.speed {
                return Some((
                    ViolationDetectionState::Overspeeding(OverspeedingState {
                        total_datapoints: 1,
                        avg_speed_record: VecDeque::from([(current_speed.inner(), 1)]),
                    }),
                    None,
                ));
            }
            None
        }
        DetectionConfig::RouteDeviationDetection(RouteDeviationConfig {
            batch_count,
            deviation_threshold,
            sample_size,
        }) => {
            if let Some(ViolationDetectionState::RouteDeviation(RouteDeviationState {
                deviation_distance, // update this value down in the code below
                mut total_datapoints,
                avg_deviation_record,
            })) = state
            {
                if let Some(prev_tuple) = avg_deviation_record.back() {
                    let max_batch_size = sample_size.div_ceil(batch_count);
                    let prev_avg_point = prev_tuple.0.clone();
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
                            let mut copy_list = avg_deviation_record.clone();
                            copy_list.pop_back();
                            copy_list.push_back((current_avg_point, prev_batch_datapoints + 1));
                            total_datapoints += 1;
                            if total_datapoints == sample_size as u64 {
                                if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                                    if let Some((true, distance)) = check_deviated(
                                        &copy_list,
                                        deviation_threshold,
                                        context.route,
                                    ) {
                                        return Some((
                                            ViolationDetectionState::RouteDeviation(
                                                RouteDeviationState {
                                                    total_datapoints,
                                                    avg_deviation_record: copy_list,
                                                    deviation_distance: distance,
                                                },
                                            ),
                                            Some(false),
                                        ));
                                    } else if let Some((false, distance)) = check_deviated(
                                        &copy_list,
                                        deviation_threshold,
                                        context.route,
                                    ) {
                                        return Some((
                                            ViolationDetectionState::RouteDeviation(
                                                RouteDeviationState {
                                                    total_datapoints,
                                                    avg_deviation_record: copy_list,
                                                    deviation_distance: distance,
                                                },
                                            ),
                                            Some(true),
                                        ));
                                    }
                                }
                            }
                            return Some((
                                ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                    total_datapoints,
                                    avg_deviation_record: copy_list,
                                    deviation_distance,
                                }),
                                None,
                            ));
                        } else {
                            let mut copy_list = avg_deviation_record.clone();
                            copy_list.push_back((context.location.clone(), 1));
                            total_datapoints += 1;
                            return Some((
                                ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                    total_datapoints,
                                    avg_deviation_record: copy_list,
                                    deviation_distance,
                                }),
                                None,
                            ));
                        }
                    } else {
                        let mut copy_list = avg_deviation_record.clone();
                        total_datapoints -= max_batch_size as u64;
                        copy_list.pop_front();
                        if let Some(prev_tuple) = copy_list.back() {
                            let prev_average_point = prev_tuple.0.clone();
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
                                let mut copy_list = avg_deviation_record.clone();
                                copy_list.pop_back();
                                copy_list.push_back((current_avg_point, prev_batch_size + 1));
                                total_datapoints += 1;
                                if total_datapoints == sample_size as u64 {
                                    if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                                        if let Some((true, distance)) = check_deviated(
                                            &copy_list,
                                            deviation_threshold,
                                            context.route,
                                        ) {
                                            return Some((
                                                ViolationDetectionState::RouteDeviation(
                                                    RouteDeviationState {
                                                        total_datapoints,
                                                        avg_deviation_record: copy_list,
                                                        deviation_distance: distance,
                                                    },
                                                ),
                                                Some(false),
                                            ));
                                        } else if let Some((false, distance)) = check_deviated(
                                            &copy_list,
                                            deviation_threshold,
                                            context.route,
                                        ) {
                                            return Some((
                                                ViolationDetectionState::RouteDeviation(
                                                    RouteDeviationState {
                                                        total_datapoints,
                                                        avg_deviation_record: copy_list,
                                                        deviation_distance: distance,
                                                    },
                                                ),
                                                Some(true),
                                            ));
                                        }
                                    }
                                }
                                return Some((
                                    ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                        total_datapoints,
                                        avg_deviation_record: copy_list,
                                        deviation_distance,
                                    }),
                                    None,
                                ));
                            } else {
                                copy_list.push_back((context.location.clone(), 1));
                                total_datapoints += 1;
                                return Some((
                                    ViolationDetectionState::RouteDeviation(RouteDeviationState {
                                        total_datapoints,
                                        avg_deviation_record: copy_list,
                                        deviation_distance,
                                    }),
                                    None,
                                ));
                            }
                        }
                    }
                }
            }
            Some((
                ViolationDetectionState::RouteDeviation(RouteDeviationState {
                    total_datapoints: 1,
                    avg_deviation_record: VecDeque::from([(context.location.clone(), 1)]),
                    deviation_distance: 0.0,
                }),
                None,
            ))
        }
        DetectionConfig::StoppedDetection(StoppedDetectionConfig {
            batch_count,
            max_eligible_speed: _,
            max_eligible_distance,
            sample_size,
        }) => handle_stop_or_safety_check(
            state,
            context,
            sample_size,
            batch_count,
            max_eligible_distance,
            true,
            false,
        ),
        DetectionConfig::OppositeDirectionDetection(OppositeDirectionConfig {}) => {
            if let Some(ViolationDetectionState::OppositeDirection(OppositeDirectionState {
                total_datapoints,
                expected_count,
            })) = state
            {
                if let Some(RideInfo::Bus { .. }) = &context.ride_info {
                    if let Some(route) = context.route {
                        let upcoming_stops =
                            get_upcoming_stops_by_route_code(None, route, &context.location);
                        if let Ok(upcoming) = upcoming_stops {
                            let count = upcoming.len() as u64;
                            if count <= expected_count {
                                return Some((
                                    ViolationDetectionState::OppositeDirection(
                                        OppositeDirectionState {
                                            total_datapoints: total_datapoints + 1,
                                            expected_count: count,
                                        },
                                    ),
                                    Some(true),
                                ));
                            } else {
                                return Some((
                                    ViolationDetectionState::OppositeDirection(
                                        OppositeDirectionState {
                                            total_datapoints: total_datapoints + 1,
                                            expected_count,
                                        },
                                    ),
                                    Some(false),
                                ));
                            }
                        }
                        return Some((
                            ViolationDetectionState::OppositeDirection(OppositeDirectionState {
                                total_datapoints: total_datapoints + 1,
                                expected_count,
                            }),
                            None,
                        ));
                    }
                }
            }
            Some((
                ViolationDetectionState::OppositeDirection(OppositeDirectionState {
                    total_datapoints: 0,
                    expected_count: 1e6 as u64,
                }),
                None,
            ))
        }
        DetectionConfig::TripNotStartedDetection(TripNotStartedConfig {
            deviation_threshold: _,
            sample_size: _,
            batch_count: _,
        }) => Some((
            ViolationDetectionState::TripNotStarted(TripNotStartedState {
                total_datapoints: 0,
                avg_coord_mean: VecDeque::new(),
            }),
            Some(false),
        )),
        DetectionConfig::SafetyCheckDetection(SafetyCheckConfig {
            batch_count,
            sample_size,
            max_eligible_speed: _,
            max_eligible_distance,
        }) => handle_stop_or_safety_check(
            state,
            context,
            sample_size,
            batch_count,
            max_eligible_distance,
            true,
            true,
        ),
        DetectionConfig::RideStopReachedDetection(RideStopReachedConfig {
            sample_size,
            batch_count,
            stop_reach_threshold,
            min_stop_duration,
        }) => handle_ride_stop_reached_check(
            state,
            context,
            sample_size,
            batch_count,
            stop_reach_threshold,
            min_stop_duration,
            true,
        ),
    }
}

fn get_triggered_state_req(
    curr_violation_state: Option<&ViolationDetectionState>,
    curr_anti_violation_state: Option<&ViolationDetectionState>,
    is_violated: Option<DetectionStatus>,
    context: DetectionContext,
) -> Option<ViolationDetectionReq> {
    match is_violated {
        Some(DetectionStatus::Violated) => match curr_violation_state {
            Some(ViolationDetectionState::Overspeeding(OverspeedingState {
                avg_speed_record,
                ..
            })) => {
                let average_speed = avg_speed_record.iter().map(|c| c.0).sum::<f64>()
                    / avg_speed_record.len() as f64;
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: true,
                    detection_data: DetectionData::OverSpeedingDetection(
                        OverSpeedingDetectionData {
                            location: context.location,
                            speed: average_speed,
                        },
                    ),
                })
            }
            Some(ViolationDetectionState::RouteDeviation(RouteDeviationState {
                deviation_distance,
                ..
            })) => Some(ViolationDetectionReq {
                ride_id: context.ride_id,
                driver_id: context.driver_id,
                is_violated: true,
                detection_data: DetectionData::RouteDeviationDetection(
                    RouteDeviationDetectionData {
                        location: context.location,
                        distance: *deviation_distance,
                    },
                ),
            }),
            Some(ViolationDetectionState::StopDetection(StopDetectionState { .. })) => {
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: true,
                    detection_data: DetectionData::StoppedDetection(StoppedDetectionData {
                        location: context.location,
                    }),
                })
            }
            Some(ViolationDetectionState::OppositeDirection(OppositeDirectionState { .. })) => {
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: true,
                    detection_data: DetectionData::OppositeDirectionDetection(
                        OppositeDirectionDetectionData {
                            location: context.location,
                        },
                    ),
                })
            }
            Some(ViolationDetectionState::SafetyCheck(SafetyCheckState { .. })) => {
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: true,
                    detection_data: DetectionData::SafetyCheckDetection(SafetyCheckDetectionData {
                        location: context.location,
                    }),
                })
            }
            Some(ViolationDetectionState::RideStopReached(RideStopReachedState {
                reached_stops,
                current_stop_index: _,
                ..
            })) => {
                if let Some(ride_stops) = context.ride_stops {
                    let last_reached_stop_index = if !reached_stops.is_empty() {
                        reached_stops
                            .last()
                            .and_then(|stop_str| stop_str.strip_prefix("stop_"))
                            .and_then(|index_str| index_str.parse::<usize>().ok())
                            .unwrap_or(0)
                    } else {
                        0
                    };

                    if last_reached_stop_index < ride_stops.len() {
                        let reached_stop = &ride_stops[last_reached_stop_index];
                        Some(ViolationDetectionReq {
                            ride_id: context.ride_id,
                            driver_id: context.driver_id,
                            is_violated: true,
                            detection_data: DetectionData::RideStopReachedDetection(
                                RideStopReachedDetectionData {
                                    location: reached_stop.clone(),
                                    stop_name: format!("Stop {}", last_reached_stop_index + 1),
                                    stop_code: format!("STOP_{}", last_reached_stop_index + 1),
                                    stop_index: last_reached_stop_index,
                                    reached_at: context.timestamp,
                                },
                            ),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        },
        Some(DetectionStatus::AntiViolated) => match curr_anti_violation_state {
            Some(ViolationDetectionState::Overspeeding(OverspeedingState {
                avg_speed_record,
                ..
            })) => {
                let average_speed = avg_speed_record.iter().map(|c| c.0).sum::<f64>()
                    / avg_speed_record.len() as f64;
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: false,
                    detection_data: DetectionData::OverSpeedingDetection(
                        OverSpeedingDetectionData {
                            location: context.location,
                            speed: average_speed,
                        },
                    ),
                })
            }
            Some(ViolationDetectionState::RouteDeviation(RouteDeviationState {
                deviation_distance,
                ..
            })) => Some(ViolationDetectionReq {
                ride_id: context.ride_id,
                driver_id: context.driver_id,
                is_violated: false,
                detection_data: DetectionData::RouteDeviationDetection(
                    RouteDeviationDetectionData {
                        location: context.location,
                        distance: *deviation_distance,
                    },
                ),
            }),
            Some(ViolationDetectionState::StopDetection(StopDetectionState { .. })) => {
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: false,
                    detection_data: DetectionData::StoppedDetection(StoppedDetectionData {
                        location: context.location,
                    }),
                })
            }
            Some(ViolationDetectionState::OppositeDirection(OppositeDirectionState { .. })) => {
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: false,
                    detection_data: DetectionData::OppositeDirectionDetection(
                        OppositeDirectionDetectionData {
                            location: context.location,
                        },
                    ),
                })
            }
            Some(ViolationDetectionState::SafetyCheck(SafetyCheckState { .. })) => {
                Some(ViolationDetectionReq {
                    ride_id: context.ride_id,
                    driver_id: context.driver_id,
                    is_violated: false,
                    detection_data: DetectionData::SafetyCheckDetection(SafetyCheckDetectionData {
                        location: context.location,
                    }),
                })
            }
            Some(ViolationDetectionState::RideStopReached(RideStopReachedState {
                reached_stops: _,
                current_stop_index,
                ..
            })) => {
                if let Some(ride_stops) = context.ride_stops {
                    if *current_stop_index < ride_stops.len() {
                        let current_stop = &ride_stops[*current_stop_index];
                        Some(ViolationDetectionReq {
                            ride_id: context.ride_id,
                            driver_id: context.driver_id,
                            is_violated: false,
                            detection_data: DetectionData::RideStopReachedDetection(
                                RideStopReachedDetectionData {
                                    location: current_stop.clone(),
                                    stop_name: format!("Stop {}", current_stop_index + 1),
                                    stop_code: format!("STOP_{}", current_stop_index + 1),
                                    stop_index: *current_stop_index,
                                    reached_at: context.timestamp,
                                },
                            ),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn violation_trigger_decision_tree(
    prev_violation_trigger: Option<&DetectionStatus>,
    curr_violation_trigger: Option<bool>,
    curr_anti_violation_trigger: Option<bool>,
) -> Option<DetectionStatus> {
    match (
        prev_violation_trigger,
        curr_violation_trigger,
        curr_anti_violation_trigger,
    ) {
        (None, _, _) => Some(DetectionStatus::ContinuedAntiViolation),
        (Some(DetectionStatus::Violated), None, None) => Some(DetectionStatus::ContinuedViolation),
        (Some(DetectionStatus::AntiViolated), None, None) => {
            Some(DetectionStatus::ContinuedAntiViolation)
        }
        (a, None, None) => a.cloned(),
        (Some(DetectionStatus::Violated), _, Some(true)) => Some(DetectionStatus::AntiViolated),
        (Some(DetectionStatus::Violated), _, Some(false)) => {
            Some(DetectionStatus::ContinuedViolation)
        }
        (Some(DetectionStatus::AntiViolated), Some(true), _) => Some(DetectionStatus::Violated),
        (Some(DetectionStatus::AntiViolated), Some(false), _) => {
            Some(DetectionStatus::ContinuedAntiViolation)
        }
        (Some(DetectionStatus::ContinuedAntiViolation), Some(true), _) => {
            Some(DetectionStatus::Violated)
        }
        (Some(DetectionStatus::ContinuedAntiViolation), Some(false), _) => {
            Some(DetectionStatus::ContinuedAntiViolation)
        }
        (Some(DetectionStatus::ContinuedViolation), _, Some(true)) => {
            Some(DetectionStatus::AntiViolated)
        }
        (Some(DetectionStatus::ContinuedViolation), _, Some(false)) => {
            Some(DetectionStatus::ContinuedViolation)
        }
        (a, b, c) => {
            warn! {"Unexpected alert status, {:?} {:?} {:?}", a,b,c};
            None
        }
    }
}

fn check_trip_not_started(
    coord_list: &VecDeque<(Point, u32)>,
    deviation_threshold: u32,
    points: Option<&Route>,
) -> Option<bool> {
    if let Some(route) = points {
        let points = route
            .waypoints
            .iter()
            .map(|w| w.coordinate.clone())
            .collect::<Vec<Point>>();

        let mut previous_distance = 1e6_f64;
        let mut index_projected = -1;
        for (point, _) in coord_list.iter() {
            if let Some(projected_point) = find_closest_point_on_route(point, points.clone()) {
                let waypoint_info = if projected_point.projection_point_to_line_end_distance
                    > projected_point.projection_point_to_line_start_distance
                {
                    route.waypoints[projected_point.segment_index as usize].clone()
                } else {
                    route.waypoints[projected_point.segment_index as usize + 1].clone()
                };
                let current_distance = waypoint_info
                    .stop
                    .distance_to_upcoming_intermediate_stop
                    .inner()
                    .into();
                if index_projected != -1 {
                    if index_projected != waypoint_info.stop.stop_idx as i32 {
                        previous_distance = current_distance;
                        index_projected = waypoint_info.stop.stop_idx as i32;
                    } else {
                        if current_distance > previous_distance {
                            return Some(false);
                        }
                        previous_distance = current_distance;
                        index_projected = waypoint_info.stop.stop_idx as i32;
                    }
                } else {
                    previous_distance = current_distance;
                    index_projected = waypoint_info.stop.stop_idx as i32;
                }
            }
        }
        // Check each point's distance from the route
        for (point, _) in coord_list.iter() {
            let projected_point = find_closest_point_on_route(point, points.clone());
            if let Some(projected_point) = projected_point {
                let distance = distance_between_in_meters(point, &projected_point.projection_point);
                if distance > deviation_threshold as f64 {
                    return Some(false);
                }
            }
        }
        return Some(true);
    }
    None
}
