/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::common::utils::{
    distance_between_in_meters, get_bucket_from_timestamp, get_bucket_weightage_from_timestamp,
};
use crate::environment::StopDetectionConfig;

/// Filters driver locations based on bucket weightage, including both current and previous bucket.
///
/// # Arguments
///
/// * `locations` - A slice of `DriverLocation` representing the driver's location history.
/// * `bucket_size` - The size of the bucket in terms of time (e.g., seconds).
/// * `curr_bucket` - The current bucket based on the latest driver's location timestamp.
/// * `curr_bucket_capacity_based_on_weightage` - The weightage used to limit the number of points in the current bucket.
///
/// # Returns
///
/// A vector containing filtered driver locations based on the bucket weightage (current and previous buckets).
fn filter_locations_based_on_bucket_weightage(
    locations: &[DriverLocation],
    bucket_size: u64,
    curr_bucket: u64,
    curr_bucket_capacity_based_on_weightage: u64,
) -> Vec<DriverLocation> {
    let prev_bucket_capacity_based_on_weightage =
        std::cmp::max(0, bucket_size - curr_bucket_capacity_based_on_weightage);

    // Using fold to filter locations
    let (mut filtered_locations, _, _) = locations.iter().rev().fold(
        (
            vec![],
            curr_bucket_capacity_based_on_weightage,
            prev_bucket_capacity_based_on_weightage,
        ),
        |(
            mut locations,
            curr_bucket_capacity_based_on_weightage,
            prev_bucket_capacity_based_on_weightage,
        ),
         location| {
            let bucket = get_bucket_from_timestamp(&bucket_size, location.timestamp);
            if bucket == curr_bucket {
                if curr_bucket_capacity_based_on_weightage > 0 {
                    locations.push(location.to_owned());
                    (
                        locations,
                        curr_bucket_capacity_based_on_weightage - 1,
                        prev_bucket_capacity_based_on_weightage,
                    )
                } else {
                    locations.push(location.to_owned());
                    (
                        locations,
                        curr_bucket_capacity_based_on_weightage,
                        prev_bucket_capacity_based_on_weightage - 1,
                    )
                }
            } else if bucket == curr_bucket - 1 && prev_bucket_capacity_based_on_weightage > 0 {
                locations.push(location.to_owned());
                (
                    locations,
                    curr_bucket_capacity_based_on_weightage,
                    prev_bucket_capacity_based_on_weightage - 1,
                )
            } else {
                (
                    locations,
                    curr_bucket_capacity_based_on_weightage,
                    prev_bucket_capacity_based_on_weightage,
                )
            }
        },
    );

    filtered_locations.reverse(); // Ensure locations are returned in correct order
    filtered_locations
}

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
fn calculate_mean_location(locations: &[DriverLocation], total_points: usize) -> Point {
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
/// * `config` - Configuration for stop detection.
///
/// # Returns
///
/// A boolean indicating whether the stop has been detected based on the radius threshold.
fn is_stop_detected(
    mean_location: &Point,
    total_points: usize,
    latest_location: &Point,
    config: &StopDetectionConfig,
) -> bool {
    let distance = distance_between_in_meters(mean_location, latest_location);
    distance < config.radius_threshold_meters as f64
        && total_points >= config.min_points_within_radius_threshold
}

/// Detects whether the driver has stopped based on a sliding window of driver locations and the stop detection configuration.
///
/// # Arguments
///
/// * `driver_ride_status` - The current ride status of the driver (must be `NEW` to process).
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
    driver_ride_status: Option<&RideStatus>,
    stop_detection: Option<StopDetection>,
    latest_driver_location: DriverLocation,
    config: &StopDetectionConfig,
) -> (Option<Point>, Option<StopDetection>) {
    if driver_ride_status != Some(&RideStatus::NEW) {
        return (None, None);
    }

    // Determine the current time bucket and the associated weightage
    let curr_bucket = get_bucket_from_timestamp(
        &config.duration_threshold_seconds,
        latest_driver_location.timestamp,
    );

    let curr_bucket_weightage = get_bucket_weightage_from_timestamp(
        &config.duration_threshold_seconds,
        latest_driver_location.timestamp,
    );

    let curr_bucket_max_points_to_consider =
        curr_bucket_weightage * config.min_points_within_radius_threshold as u64;

    if let Some(stop_detection) = stop_detection {
        let mut locations = filter_locations_based_on_bucket_weightage(
            &stop_detection.locations,
            config.duration_threshold_seconds,
            curr_bucket,
            curr_bucket_max_points_to_consider,
        );

        let mean_location = calculate_mean_location(&locations, locations.len());

        let stop_detected = if is_stop_detected(
            &mean_location,
            locations.len(),
            &latest_driver_location.location,
            config,
        ) {
            locations.clear(); // Clear the history if a stop is detected
            Some(mean_location)
        } else {
            locations.push(latest_driver_location); // Add the latest location
            None
        };

        (stop_detected, Some(StopDetection { locations }))
    } else {
        // First time: start with the latest location
        (
            None,
            Some(StopDetection {
                locations: vec![latest_driver_location],
            }),
        )
    }
}
