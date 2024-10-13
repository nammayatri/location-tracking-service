/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::common::utils::{distance_between_in_meters, get_bucket_from_timestamp};
use crate::domain::types::ui::location::UpdateDriverLocationRequest;
use crate::environment::StopDetectionConfig;

/// Sums the location points within the same timestamp bucket.
///
/// # Arguments
///
/// * `locations` - A slice of `Location` objects to be processed.
/// * `bucket_size` - The target bucket size.
/// * `initial` - The initial sum and count of the geographical coordinates (lat/lon).
///
/// # Returns
///
/// A tuple containing the updated sum of location points and the total number of points in the bucket.
fn sum_locations_in_bucket(
    locations: &[UpdateDriverLocationRequest],
    bucket_size: u64,
    curr_bucket: u64,
    initial: (Point, usize),
) -> (Point, usize) {
    locations
        .iter()
        .fold(initial, |(loc_sum, count), location| {
            let bucket = get_bucket_from_timestamp(&bucket_size, location.ts);
            if bucket == curr_bucket {
                (
                    Point {
                        lat: Latitude(loc_sum.lat.inner() + location.pt.lat.inner()),
                        lon: Longitude(loc_sum.lon.inner() + location.pt.lon.inner()),
                    },
                    count + 1,
                )
            } else {
                (loc_sum, count)
            }
        })
}

/// Calculates the mean location (latitude and longitude) based on the summed locations and total points.
///
/// # Arguments
///
/// * `location_sum` - The sum of latitude and longitude values.
/// * `total_points` - The total number of points used to calculate the mean.
///
/// # Returns
///
/// A `Point` representing the mean location.
fn calculate_mean_location(location_sum: Point, total_points: usize) -> Point {
    Point {
        lat: Latitude(location_sum.lat.inner() / total_points as f64),
        lon: Longitude(location_sum.lon.inner() / total_points as f64),
    }
}

/// Determines whether a stop is detected based on distance and the number of points within the radius.
///
/// # Arguments
///
/// * `mean_location` - The mean location of the points.
/// * `latest_location` - The latest location of the driver.
/// * `total_points_in_bucket` - The total number of points in the current bucket.
/// * `config` - Configuration parameters for stop detection.
///
/// # Returns
///
/// A boolean indicating whether a stop has been detected.
fn is_stop_detected(
    mean_location: &Point,
    latest_location: &Point,
    total_points_in_bucket: usize,
    config: &StopDetectionConfig,
) -> bool {
    let distance = distance_between_in_meters(mean_location, latest_location);
    distance < config.radius_threshold_meters as f64
        && total_points_in_bucket >= config.min_points_within_radius_threshold
}

/// Detects whether a stop has occurred based on driver locations and stop detection configuration.
///
/// # Arguments
///
/// * `driver_ride_status` - The current ride status of the driver.
/// * `stop_detection` - Optional stop detection data, if already available.
/// * `locations` - A slice of `Location` objects representing driver locations.
/// * `latest_driver_location` - The latest driver location.
/// * `config` - Configuration parameters for stop detection.
///
/// # Returns
///
/// A tuple containing an `Option<(Point, usize)>` with the detected stop location and number of points, and an `Option<StopDetection>` with the updated stop detection data.
pub fn detect_stop(
    driver_ride_status: Option<&RideStatus>,
    stop_detection: Option<StopDetection>,
    locations: &[UpdateDriverLocationRequest],
    latest_driver_location: &UpdateDriverLocationRequest,
    config: &StopDetectionConfig,
) -> (Option<(Point, usize)>, Option<StopDetection>) {
    if driver_ride_status != Some(&RideStatus::NEW) {
        return (None, None);
    }

    let duration_bucket = get_bucket_from_timestamp(
        &config.duration_threshold_seconds,
        latest_driver_location.ts,
    );

    if let Some(stop_detection) = stop_detection {
        // Sum the locations for the current bucket
        let (location_sum, total_points_in_bucket) = sum_locations_in_bucket(
            locations,
            config.duration_threshold_seconds,
            stop_detection.duration_bucket,
            (
                stop_detection.location_sum,
                stop_detection.total_points_in_bucket,
            ),
        );

        // Check if the bucket has changed
        if stop_detection.duration_bucket != duration_bucket {
            let mean_location = calculate_mean_location(location_sum, total_points_in_bucket);

            // Determine if a stop is detected
            let stop_detected = if is_stop_detected(
                &mean_location,
                &latest_driver_location.pt,
                total_points_in_bucket,
                config,
            ) {
                Some((mean_location, total_points_in_bucket))
            } else {
                None
            };

            // Sum locations for the new bucket after detecting the stop
            let (new_location_sum, new_total_points) = sum_locations_in_bucket(
                locations,
                config.duration_threshold_seconds,
                duration_bucket,
                (
                    Point {
                        lat: Latitude(0.0),
                        lon: Longitude(0.0),
                    },
                    0,
                ),
            );

            (
                stop_detected,
                Some(StopDetection {
                    location_sum: new_location_sum,
                    duration_bucket,
                    total_points_in_bucket: new_total_points,
                }),
            )
        } else {
            // Update the stop detection without changing the bucket
            (
                None,
                Some(StopDetection {
                    location_sum,
                    duration_bucket,
                    total_points_in_bucket,
                }),
            )
        }
    } else {
        // Case where stop detection has just started
        let (location_sum, total_points_in_bucket) = sum_locations_in_bucket(
            locations,
            config.duration_threshold_seconds,
            duration_bucket,
            (
                Point {
                    lat: Latitude(0.0),
                    lon: Longitude(0.0),
                },
                0,
            ),
        );

        (
            None,
            Some(StopDetection {
                location_sum,
                duration_bucket,
                total_points_in_bucket,
            }),
        )
    }
}
