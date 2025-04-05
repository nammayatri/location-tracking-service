/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use tracing::warn;

use crate::{
    common::types::*,
    outbound::types::{DetectionData, OverSpeedingDetectionData, ViolationDetectionReq},
};

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
            .map(|(_, trigger)| trigger),
        anti_violation_state_and_trigger
            .as_ref()
            .map(|(_, trigger)| trigger),
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
) -> Option<(ViolationDetectionState, bool)> {
    match config.detection_config {
        DetectionConfig::OverspeedingDetection(OverspeedingConfig {
            sample_size,
            speed_limit,
        }) => {
            // if state present then use it
            if config.enabled_on_pick_up {
                if let Some(ViolationDetectionState::Overspeeding(OverspeedingState {
                    avg_speed: p_average_speed,
                    total_datapoints: p_total_datapoints,
                })) = state
                {
                    if p_total_datapoints >= sample_size.into() {
                        // if we have already sufficient data points, then reset the state
                        if let Some(current_speed) = context.speed {
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    avg_speed: current_speed.inner(),
                                    total_datapoints: 1,
                                }),
                                false,
                            ));
                        }
                    } else {
                        // if less than sample size, simply calculate average and return
                        // in case we reach the sample size by this addition and cross threshold
                        // then send the alert
                        if let Some(current_speed) = context.speed {
                            let current_total_datapoints = p_total_datapoints + 1;
                            let current_avg_speed = (p_average_speed * p_total_datapoints as f64
                                + current_speed.inner())
                                / (current_total_datapoints as f64);

                            if current_total_datapoints == sample_size as u64
                                && current_avg_speed > speed_limit
                            {
                                return Some((
                                    ViolationDetectionState::Overspeeding(OverspeedingState {
                                        avg_speed: current_avg_speed,
                                        total_datapoints: current_total_datapoints,
                                    }),
                                    true,
                                ));
                            }
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    avg_speed: current_avg_speed,
                                    total_datapoints: current_total_datapoints,
                                }),
                                false,
                            ));
                        }
                    }
                } else {
                    // set the state with the current
                    if let Some(current_speed) = context.speed {
                        return Some((
                            ViolationDetectionState::Overspeeding(OverspeedingState {
                                avg_speed: current_speed.inner(),
                                total_datapoints: 1,
                            }),
                            false,
                        ));
                    }
                }
            }
            None
        }
        DetectionConfig::RouteDeviationDetection { .. } => None,
        DetectionConfig::StoppedDetection { .. } => None,
    }
}

fn anti_violation_check(
    config: &ViolationDetectionConfig,
    context: &DetectionContext,
    state: Option<ViolationDetectionState>,
) -> Option<(ViolationDetectionState, bool)> {
    match config.detection_config {
        DetectionConfig::OverspeedingDetection(OverspeedingConfig {
            sample_size,
            speed_limit,
        }) => {
            if config.enabled_on_pick_up {
                if let Some(ViolationDetectionState::Overspeeding(OverspeedingState {
                    avg_speed: p_average_speed,
                    total_datapoints: p_total_datapoints,
                })) = state
                {
                    if p_total_datapoints >= sample_size.into() {
                        if let Some(current_speed) = context.speed {
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    avg_speed: current_speed.inner(),
                                    total_datapoints: 1,
                                }),
                                false,
                            ));
                        }
                    } else if let Some(current_speed) = context.speed {
                        let current_total_datapoints = p_total_datapoints + 1;
                        let current_avg_speed = (p_average_speed * p_total_datapoints as f64
                            + current_speed.inner())
                            / (current_total_datapoints as f64);

                        if current_total_datapoints == sample_size as u64
                            && current_avg_speed <= speed_limit
                        {
                            return Some((
                                ViolationDetectionState::Overspeeding(OverspeedingState {
                                    avg_speed: current_avg_speed,
                                    total_datapoints: current_total_datapoints,
                                }),
                                true,
                            ));
                        }

                        return Some((
                            ViolationDetectionState::Overspeeding(OverspeedingState {
                                avg_speed: current_avg_speed,
                                total_datapoints: current_total_datapoints,
                            }),
                            false,
                        ));
                    }
                } else if let Some(current_speed) = context.speed {
                    return Some((
                        ViolationDetectionState::Overspeeding(OverspeedingState {
                            avg_speed: current_speed.inner(),
                            total_datapoints: 1,
                        }),
                        false,
                    ));
                }
            }
            None
        }
        DetectionConfig::RouteDeviationDetection { .. } => None,
        DetectionConfig::StoppedDetection { .. } => None,
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
                avg_speed, ..
            })) => Some(ViolationDetectionReq {
                ride_id: context.ride_id,
                driver_id: context.driver_id,
                is_violated: true,
                detection_data: DetectionData::OverSpeedingDetection(OverSpeedingDetectionData {
                    location: context.location,
                    speed: *avg_speed,
                }),
            }),
            _ => None,
        },
        Some(DetectionStatus::AntiViolated) => match curr_anti_violation_state {
            Some(ViolationDetectionState::Overspeeding(OverspeedingState {
                avg_speed, ..
            })) => Some(ViolationDetectionReq {
                ride_id: context.ride_id,
                driver_id: context.driver_id,
                is_violated: false,
                detection_data: DetectionData::OverSpeedingDetection(OverSpeedingDetectionData {
                    location: context.location,
                    speed: *avg_speed,
                }),
            }),
            _ => None,
        },
        _ => None,
    }
}

fn violation_trigger_decision_tree(
    prev_violation_trigger: Option<&DetectionStatus>,
    curr_violation_trigger: Option<&bool>,
    curr_anti_violation_trigger: Option<&bool>,
) -> Option<DetectionStatus> {
    match (
        prev_violation_trigger,
        curr_violation_trigger,
        curr_anti_violation_trigger,
    ) {
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
        (None, _, _) => Some(DetectionStatus::ContinuedAntiViolation),
        (a, b, c) => {
            warn! {"Unexpected alert status, {:?} {:?} {:?}", a,b,c};
            None
        }
    }
}
