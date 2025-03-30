/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::detection::{
    DetectionConfig, DetectionContext, DetectionHandler, DetectionResult,
};
use crate::common::types::*;
use crate::common::utils::distance_between_in_meters;
use crate::environment::StopDetectionConfig;
use std::collections::VecDeque;

pub struct StopDetectionHandler {
    config: DetectionConfig,
    stop_config: StopDetectionConfig,
    locations: VecDeque<DriverLocation>,
}

impl StopDetectionHandler {
    pub fn new(config: DetectionConfig, stop_config: StopDetectionConfig) -> Self {
        Self {
            config,
            stop_config,
            locations: VecDeque::new(),
        }
    }

    fn calculate_mean_location(&self) -> Point {
        let location_sum = self.locations.iter().fold(
            Point {
                lat: Latitude(0.0),
                lon: Longitude(0.0),
            },
            |location_sum, location| Point {
                lat: Latitude(location_sum.lat.inner() + location.location.lat.inner()),
                lon: Longitude(location_sum.lon.inner() + location.location.lon.inner()),
            },
        );
        Point {
            lat: Latitude(location_sum.lat.inner() / self.locations.len() as f64),
            lon: Longitude(location_sum.lon.inner() / self.locations.len() as f64),
        }
    }

    fn is_stop_detected(
        &self,
        mean_location: &Point,
        latest_location: &Point,
        speed: Option<SpeedInMeterPerSecond>,
    ) -> bool {
        let distance = distance_between_in_meters(mean_location, latest_location);
        distance < self.stop_config.radius_threshold_meters as f64
            && self.locations.len() >= self.stop_config.min_points_within_radius_threshold
            && speed.is_some_and(|speed| {
                speed <= SpeedInMeterPerSecond(self.stop_config.max_eligible_stop_speed_threshold)
            })
    }
}

impl DetectionHandler for StopDetectionHandler {
    fn name(&self) -> &'static str {
        "stop_detection"
    }

    fn is_enabled(&self, _context: &DetectionContext) -> bool {
        self.config.enabled
    }

    fn check(&mut self, context: &DetectionContext) -> Option<DetectionResult> {
        let latest_location = DriverLocation {
            location: context.location.clone(),
            timestamp: context.timestamp,
        };

        // First, check if we have enough points
        let has_enough_points = self.locations.len() >= 3;

        if has_enough_points {
            // Calculate mean location
            let mean_location = self.calculate_mean_location();

            // Check for stop
            if self.is_stop_detected(&mean_location, &context.location, context.speed) {
                // Clear locations and return result
                self.locations.clear();
                return Some(DetectionResult::StopDetected {
                    location: mean_location,
                    timestamp: context.timestamp,
                });
            }

            // Remove oldest point and add new one
            self.locations.pop_front();
            self.locations.push_back(latest_location);
        } else {
            // Add new point
            self.locations.push_back(latest_location);
        }

        None
    }
}
