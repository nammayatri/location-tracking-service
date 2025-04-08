/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::common::utils::distance_between_in_meters;
use crate::environment::StopDetectionConfig;
use std::collections::VecDeque;

/// Calculates the mean location (latitude and longitude) based on a set of driver locations.
///
/// # Arguments
///
/// * `locations` - A slice of `DriverLocation` to compute the mean from.
/// * `total_points` - The total number of points used to calculate the mean.
///
/// # Returns
///
/// A `Point` representing the mean location.
fn calculate_mean_location(locations: &VecDeque<DriverLocation>, total_points: usize) -> Point {
    let location_sum = locations.iter().fold(
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
        lat: Latitude(location_sum.lat.inner() / total_points as f64),
        lon: Longitude(location_sum.lon.inner() / total_points as f64),
    }
}

/// Determines whether a stop has been detected by comparing the mean location to the latest location.
///
/// # Arguments
///
/// * `mean_location` - The calculated mean location of driver pings.
/// * `total_points` - The total number of points used to calculate the mean.
/// * `latest_location` - The most recent driver location.
/// * `speed` - The speed in m/s of the most recent driver location.
/// * `config` - Configuration for stop detection.
///
/// # Returns
///
/// A boolean indicating whether the stop has been detected based on the radius threshold.
fn is_stop_detected(
    mean_location: &Point,
    total_points: usize,
    latest_location: &Point,
    speed: Option<SpeedInMeterPerSecond>,
    config: &StopDetectionConfig,
) -> bool {
    let distance = distance_between_in_meters(mean_location, latest_location);
    distance < config.radius_threshold_meters as f64
        && total_points >= config.min_points_within_radius_threshold
        && config
            .max_eligible_stop_speed_threshold
            .is_none_or(|max_speed| {
                speed.is_some_and(|SpeedInMeterPerSecond(speed)| speed <= max_speed)
            })
}

/// Detects whether the driver has stopped based on a sliding window of driver locations and the stop detection configuration.
///
/// # Arguments
///
/// * `stop_detection` - Optional stop detection data from previous checks, if any.
/// * `latest_driver_location` - The latest known driver location.
/// * `config` - Configuration parameters for stop detection.
///
/// # Returns
///
/// A tuple:
/// * `Option<Point>` - The detected stop location, if a stop has been identified.
/// * `Option<StopDetection>` - Updated stop detection data to be used for further checks.
pub fn detect_stop(
    stop_detection: Option<StopDetection>,
    latest_driver_location: DriverLocation,
    speed: Option<SpeedInMeterPerSecond>,
    config: &StopDetectionConfig,
) -> (Option<Point>, Option<StopDetection>) {
    if let Some(mut stop_detection) = stop_detection {
        let mean_location =
            calculate_mean_location(&stop_detection.locations, stop_detection.locations.len());

        let stop_detected = if is_stop_detected(
            &mean_location,
            stop_detection.locations.len(),
            &latest_driver_location.location,
            speed,
            config,
        ) {
            // Clear the history if a stop is detected
            stop_detection.locations.clear();
            Some(mean_location)
        } else {
            if stop_detection.locations.len() >= config.min_points_within_radius_threshold {
                // Remove oldest location to always maintain a window of `min_points_within_radius_threshold`
                stop_detection.locations.pop_front();
            }
            // Add the latest location
            stop_detection.locations.push_back(latest_driver_location);
            None
        };

        (
            stop_detected,
            Some(StopDetection {
                locations: stop_detection.locations,
            }),
        )
    } else {
        (
            None,
            Some(StopDetection {
                locations: VecDeque::new(),
            }),
        )
    }
}
