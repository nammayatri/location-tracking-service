/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use crate::tools::error::AppError;
use chrono::{DateTime, Utc};
use geo::{point, Intersects};
use std::{f64::consts::PI, time::Duration};

/// Retrieves the name of the city based on latitude and longitude coordinates.
///
/// This function goes through each multi-polygon body in the provided vector,
/// checking if the given latitude and longitude intersect with any of them.
/// If an intersection is found, it retrieves the name of the region (city) associated with
/// that multi-polygon body.
///
/// # Arguments
///
/// * `lat` - Latitude coordinate.
/// * `lon` - Longitude coordinate.
/// * `polygon` - A vector of multi-polygon bodies, each associated with a city or region.
///
/// # Returns
///
/// * `Ok(CityName)` - If an intersection is found, returns the name of the city or region inside a `CityName` wrapper as a result.
/// * `Err(AppError)` - If no intersection is found, returns an error with the type `AppError::Unserviceable`, including the latitude and longitude that were checked.
pub fn get_city(
    lat: &Latitude,
    lon: &Longitude,
    polygon: &Vec<MultiPolygonBody>,
) -> Result<CityName, AppError> {
    let mut city = String::new();
    let mut intersection = false;

    let Latitude(lat) = *lat;
    let Longitude(lon) = *lon;

    for multi_polygon_body in polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: lon, y: lat));
        if intersection {
            city = multi_polygon_body.region.to_string();
            break;
        }
    }

    if intersection {
        Ok(CityName(city))
    } else {
        Err(AppError::Unserviceable(lat, lon))
    }
}

/// Checks if a merchant is blacklisted for a special zone based on their ID and location,
/// if blacklisted then their location will not be drained and will not be part of nearby driver pooling.
///
/// This function will first check if the merchant is in the blacklist, and if they are,
/// it will further check if the merchant's location intersects with any of the given multi-polygon bodies.
///
/// # Arguments
///
/// * `merchant_id` - The ID of the merchant to check.
/// * `blacklist_merchants` - A slice of blacklisted merchant IDs.
/// * `lat` - Latitude of the merchant's location.
/// * `lon` - Longitude of the merchant's location.
/// * `polygon` - A vector of multi-polygon bodies representing restricted areas.
///
/// # Returns
///
/// Returns `true` if the merchant is blacklisted and their location intersects
/// with any of the provided multi-polygon bodies. Otherwise, returns `false`.
pub fn is_blacklist_for_special_zone(
    merchant_id: &MerchantId,
    blacklist_merchants: &[MerchantId],
    lat: &Latitude,
    lon: &Longitude,
    polygon: &Vec<MultiPolygonBody>,
) -> bool {
    let blacklist_merchant = blacklist_merchants.contains(merchant_id);

    if blacklist_merchant {
        let mut intersection = false;

        let Latitude(lat) = *lat;
        let Longitude(lon) = *lon;

        for multi_polygon_body in polygon {
            intersection = multi_polygon_body
                .multipolygon
                .intersects(&point!(x: lon, y: lat));
            if intersection {
                break;
            }
        }

        intersection
    } else {
        false
    }
}

/// Computes a bucket identifier for a given timestamp based on a specified expiry duration.
///
/// The function divides the given timestamp by the `location_expiry_in_seconds` to determine
/// which "bucket" or interval the timestamp falls into. This is useful for partitioning or
/// grouping events that occur within certain time intervals.
///
/// # Arguments
///
/// * `location_expiry_in_seconds` - The duration, in seconds, that determines the length of each bucket.
/// * `TimeStamp(ts)` - A timestamp wrapped in the `TimeStamp` type.
///
/// # Returns
///
/// The bucket identifier as a `u64` value, representing which interval the timestamp belongs to.
///
/// # Examples
///
/// ```
/// let expiry_duration = 3600; // 1 hour
/// let sample_timestamp = TimeStamp(chrono::Utc::now());
///
/// let bucket = get_bucket_from_timestamp(&expiry_duration, sample_timestamp);
/// println!("Timestamp belongs to bucket: {}", bucket);
/// ```
///
/// # Notes
///
/// The function assumes that the timestamp is represented in seconds since the Unix epoch.
pub fn get_bucket_from_timestamp(bucket_expiry_in_seconds: &u64, TimeStamp(ts): TimeStamp) -> u64 {
    ts.timestamp() as u64 / bucket_expiry_in_seconds
}

pub fn get_bucket_weightage_from_timestamp(
    bucket_expiry_in_seconds: &u64,
    TimeStamp(ts): TimeStamp,
) -> u64 {
    ts.timestamp() as u64 % *bucket_expiry_in_seconds / *bucket_expiry_in_seconds
}

/// Calculates the distance between two geographical points in meters.
///
/// The function utilizes the haversine formula to compute the great-circle
/// distance between two points on the surface of a sphere, which in this case
/// is the Earth. This method provides a reliable calculation for short distances.
///
/// # Arguments
///
/// * `latlong1` - The first geographical point, represented as a `Point` with latitude and longitude.
/// * `latlong2` - The second geographical point, similarly represented.
///
/// # Returns
///
/// The calculated distance between the two points in meters.
///
/// # Examples
///
/// ```
/// let point1 = Point { lat: Latitude(34.0522), lon: Longitude(-118.2437) }; // Los Angeles
/// let point2 = Point { lat: Latitude(40.7128), lon: Longitude(-74.0060) };  // New York
///
/// let distance = distance_between_in_meters(&point1, &point2);
/// println!("Distance: {} meters", distance);
/// ```
///
/// # Notes
///
/// The function assumes the Earth as a perfect sphere with a radius of 6,371,000 meters.
/// For very precise measurements, other methods or refinements may be necessary.
pub fn distance_between_in_meters(latlong1: &Point, latlong2: &Point) -> f64 {
    // Calculating using haversine formula
    // Radius of Earth in meters
    let r: f64 = 6371000.0;

    let Latitude(lat1) = latlong1.lat;
    let Longitude(lon1) = latlong1.lon;
    let Latitude(lat2) = latlong2.lat;
    let Longitude(lon2) = latlong2.lon;

    let deg2rad = |degrees: f64| -> f64 { degrees * PI / 180.0 };

    let dlat = deg2rad(lat2 - lat1);
    let dlon = deg2rad(lon2 - lon1);

    let rlat1 = deg2rad(lat1);
    let rlat2 = deg2rad(lat2);

    let sq = |x: f64| x * x;

    // Calculated distance is real (not imaginary) when 0 <= h <= 1
    // Ideally in our use case h wouldn't go out of bounds
    let h = sq((dlat / 2.0).sin()) + rlat1.cos() * rlat2.cos() * sq((dlon / 2.0).sin());

    2.0 * r * h.sqrt().atan2((1.0 - h).sqrt())
}

/// Takes a vector of `Option<T>` and returns a vector of unwrapped `T` values, filtering out `None`.
///
/// # Examples
///
/// ```
/// let options = vec![Some(1), None, Some(2), Some(3), None];
/// let unwrapped = cat_maybes(options);
/// assert_eq!(unwrapped, vec![1, 2, 3]);
/// ```
///
/// # Type Parameters
///
/// - `T`: The type of the values contained in the `Option`.
///
/// # Arguments
///
/// - `options`: A vector of `Option<T>` that may contain `Some(T)` or `None` values.
///
/// # Returns
///
/// A vector of unwrapped `T` values, with `None` values omitted.
///
pub fn cat_maybes<T>(options: Vec<Option<T>>) -> Vec<T> {
    options.into_iter().flatten().collect()
}

/// Calculates the absolute difference in seconds between two UTC `DateTime` values.
///
/// This function takes two `DateTime<Utc>` values, `old` and `new`, and returns the absolute
/// difference in seconds between them. The difference is calculated as follows:
///
/// 1. Subtract `old` from `new` to obtain the `Duration` between them.
/// 2. Convert the duration to seconds and milliseconds.
/// 3. Return the total duration in seconds as a floating-point number, including milliseconds.
///
/// # Arguments
///
/// * `old` - The older `DateTime<Utc>` value.
/// * `new` - The newer `DateTime<Utc>` value.
///
/// # Returns
///
/// A floating-point number representing the absolute difference in seconds between `old` and `new`.
///
pub fn abs_diff_utc_as_sec(old: DateTime<Utc>, new: DateTime<Utc>) -> f64 {
    let duration = new.signed_duration_since(old);
    duration.num_seconds() as f64 + (duration.num_milliseconds() % 1000) as f64 / 1000.0
}

pub fn get_base_vehicle_type(vehicle_type: &VehicleType) -> VehicleType {
    match vehicle_type {
        VehicleType::SEDAN
        | VehicleType::TAXI
        | VehicleType::TaxiPlus
        | VehicleType::PremiumSedan
        | VehicleType::BLACK
        | VehicleType::BlackXl
        | VehicleType::SuvPlus
        | VehicleType::HeritageCab => VehicleType::SEDAN,
        VehicleType::BusAc | VehicleType::BusNonAc => VehicleType::BusAc,
        VehicleType::AutoRickshaw | VehicleType::EvAutoRickshaw => VehicleType::AutoRickshaw,
        VehicleType::BIKE | VehicleType::DeliveryBike => VehicleType::BIKE,
        _ => VehicleType::SEDAN, // Default to SEDAN for all other types
    }
}

pub fn get_upcoming_stops_by_route_code(
    route: &Route,
    point: &Point,
) -> Result<Vec<Stop>, AppError> {
    if let Some(projection) = find_closest_point_on_route(
        point,
        route
            .waypoints
            .iter()
            .map(|w| w.coordinate.to_owned())
            .collect(),
    ) {
        let (
            Stop {
                name,
                stop_code,
                coordinate,
                stop_idx,
                distance_to_upcoming_intermediate_stop:
                    Meters(distance_to_upcoming_intermediate_stop),
                duration_to_upcoming_intermediate_stop,
                stop_type,
            },
            delta,
        ) = if projection.projection_point_to_line_start_distance
            < projection.projection_point_to_line_end_distance
        {
            (
                route.waypoints[projection.segment_index as usize]
                    .stop
                    .to_owned(),
                -projection.projection_point_to_line_start_distance,
            )
        } else {
            (
                route.waypoints[projection.segment_index as usize + 1]
                    .stop
                    .to_owned(),
                projection.projection_point_to_line_end_distance,
            )
        };

        let upcoming_stop = Stop {
            name,
            stop_code,
            coordinate,
            stop_idx,
            distance_to_upcoming_intermediate_stop: Meters(
                (distance_to_upcoming_intermediate_stop as f64 + delta) as u32,
            ),
            duration_to_upcoming_intermediate_stop,
            stop_type,
        };

        let mut upcoming_stops = Vec::new();
        let mut distance_to_upcoming_intermediate_stop = None;
        let mut duration_to_upcoming_intermediate_stop = None;

        for (idx, waypoint) in route.waypoints.iter().enumerate() {
            if waypoint.stop.stop_type == StopType::IntermediateStop
                && waypoint.stop.stop_idx > upcoming_stop.stop_idx
            {
                if let (
                    Some(distance_to_upcoming_intermediate_stop),
                    Some(duration_to_upcoming_intermediate_stop),
                ) = (
                    distance_to_upcoming_intermediate_stop,
                    duration_to_upcoming_intermediate_stop,
                ) {
                    let stop = Stop {
                        name: waypoint.stop.name.to_owned(),
                        stop_code: waypoint.stop.stop_code.to_owned(),
                        coordinate: waypoint.stop.coordinate.to_owned(),
                        stop_idx: waypoint.stop.stop_idx,
                        distance_to_upcoming_intermediate_stop,
                        duration_to_upcoming_intermediate_stop,
                        stop_type: waypoint.stop.stop_type.to_owned(),
                    };
                    upcoming_stops.push(stop);
                }
            }

            if waypoint.stop.stop_type == StopType::IntermediateStop
                && waypoint.stop.stop_idx >= upcoming_stop.stop_idx
            {
                if let Some(stop) = route
                    .waypoints
                    .get(idx + 1)
                    .map(|waypoint| waypoint.stop.to_owned())
                {
                    distance_to_upcoming_intermediate_stop =
                        Some(stop.distance_to_upcoming_intermediate_stop);
                    duration_to_upcoming_intermediate_stop =
                        Some(stop.duration_to_upcoming_intermediate_stop);
                }
            }
        }

        Ok([upcoming_stop]
            .into_iter()
            .chain(upcoming_stops)
            .collect::<Vec<_>>())
    } else {
        Err(AppError::InvalidRequest(
            "Unable to find Upcoming Stops.".to_string(),
        ))
    }
}

pub fn distance(point1: &Point, point2: &Point) -> f64 {
    let dx = point1.lat.inner() - point2.lat.inner();
    let dy = point1.lon.inner() - point2.lon.inner();
    (dx * dx + dy * dy).sqrt()
}

pub fn calculate_projection_point(point: &Point, line_start: &Point, line_end: &Point) -> Point {
    let x = point.lat.inner();
    let y = point.lon.inner();
    let x1 = line_start.lat.inner();
    let y1 = line_start.lon.inner();
    let x2 = line_end.lat.inner();
    let y2 = line_end.lon.inner();

    let line_length = distance(line_start, line_end);

    // Calculate the dot product to determine where the projection falls on the line
    let dot_product = ((x - x1) * (x2 - x1) + (y - y1) * (y2 - y1)) / (line_length * line_length);

    // Clamp the dot product to ensure the projection point is on the line segment
    let clamped_dot_product = dot_product.clamp(0.0, 1.0);

    // Calculate the projection point coordinates
    let px = x1 + clamped_dot_product * (x2 - x1);
    let py = y1 + clamped_dot_product * (y2 - y1);

    // Return the projection point
    Point {
        lat: Latitude(px),
        lon: Longitude(py),
    }
}

pub fn find_closest_point_on_route(
    point: &Point,
    coordinates: Vec<Point>,
) -> Option<ProjectionPoint> {
    let mut closest_segment = ProjectionPoint {
        segment_index: -1,
        projection_point: Point {
            lat: Latitude(0.0),
            lon: Longitude(0.0),
        },
        projection_point_to_point_distance: f64::MAX,
        projection_point_to_line_start_distance: f64::MAX,
        projection_point_to_line_end_distance: f64::MAX,
    };

    for i in 0..coordinates.len() - 1 {
        let line_start = &coordinates[i];
        let line_end = &coordinates[i + 1];

        let projection_point = calculate_projection_point(point, line_start, line_end);

        let projection_point_to_point_distance =
            distance_between_in_meters(&projection_point, point);

        if projection_point_to_point_distance < closest_segment.projection_point_to_point_distance {
            let distance_to_start = distance_between_in_meters(&projection_point, line_start);
            let distance_to_end = distance_between_in_meters(&projection_point, line_end);

            closest_segment = ProjectionPoint {
                segment_index: i as i32,
                projection_point,
                projection_point_to_point_distance,
                projection_point_to_line_start_distance: distance_to_start,
                projection_point_to_line_end_distance: distance_to_end,
            };
        }
    }

    if closest_segment.projection_point_to_point_distance == f64::MAX
        || closest_segment.segment_index == -1
    {
        None
    } else {
        Some(closest_segment)
    }
}

pub fn estimated_upcoming_stops_eta(
    upcoming_stops_with_eta: Option<Vec<UpcomingStop>>,
    speed: Option<SpeedInMeterPerSecond>,
    upcoming_stops: &Vec<Stop>,
    point: &Point,
) -> Option<Vec<UpcomingStop>> {
    let now = Utc::now();
    let stop_threshold = 100.0; // TODO :: Make it configurable
    if let Some(upcoming_stops_with_eta) = upcoming_stops_with_eta {
        let delta = upcoming_stops_with_eta
            .iter()
            .find(|upcoming_stop| upcoming_stop.status == UpcomingStopStatus::Upcoming)
            .and_then(|upcoming_stop_with_eta| {
                if distance_between_in_meters(point, &upcoming_stop_with_eta.stop.coordinate)
                    < stop_threshold
                {
                    let upcoming_stop_eta = upcoming_stop_with_eta.eta.inner();
                    if now > upcoming_stop_eta {
                        Some(abs_diff_utc_as_sec(upcoming_stop_eta, now))
                    } else {
                        Some(-abs_diff_utc_as_sec(now, upcoming_stop_eta))
                    }
                } else {
                    None
                }
            });

        let mut distance_to_upcoming_stop = 0.0;
        Some(
            upcoming_stops_with_eta
                .iter()
                .map(|upcoming_stop_with_eta| {
                    if let Some(upcoming_stop) = upcoming_stops
                        .iter()
                        .find(|stop| stop.stop_idx == upcoming_stop_with_eta.stop.stop_idx)
                    {
                        let delta = if upcoming_stop_with_eta.eta.inner()
                            + Duration::from_secs(
                                upcoming_stop_with_eta.delta.map_or(0, |delta| delta as u64),
                            )
                            > now
                        {
                            upcoming_stop_with_eta.delta.map_or(0.0, |delta| delta)
                        } else {
                            let distance = distance_to_upcoming_stop
                                + upcoming_stop.distance_to_upcoming_intermediate_stop.inner()
                                    as f64;
                            distance_to_upcoming_stop += distance;
                            let speed = speed
                                .unwrap_or(SpeedInMeterPerSecond(1.0))
                                .inner()
                                .clamp(1.0, 16.66);
                            let time = distance / speed;
                            let updated_eta_delta =
                                (now - upcoming_stop_with_eta.eta.inner()).num_seconds();
                            updated_eta_delta as f64 + time
                        };

                        UpcomingStop {
                            stop: upcoming_stop.to_owned(),
                            eta: upcoming_stop_with_eta.eta,
                            delta: Some(delta),
                            status: UpcomingStopStatus::Upcoming,
                        }
                    } else {
                        UpcomingStop {
                            stop: upcoming_stop_with_eta.stop.to_owned(),
                            eta: upcoming_stop_with_eta.eta,
                            delta: upcoming_stop_with_eta.delta.or(delta),
                            status: UpcomingStopStatus::Reached,
                        }
                    }
                })
                .collect(),
        )
    } else if let Some(_upcoming_stop) = upcoming_stops.first() {
        let mut upcoming_stops_with_eta = Vec::new();
        let mut prev_stop_durations: u64 = 0;
        for stop in upcoming_stops {
            prev_stop_durations += stop.duration_to_upcoming_intermediate_stop.inner() as u64;
            let upcoming_stop = UpcomingStop {
                stop: stop.to_owned(),
                eta: TimeStamp(Utc::now() + Duration::from_secs(prev_stop_durations)),
                delta: None,
                status: UpcomingStopStatus::Upcoming,
            };
            upcoming_stops_with_eta.push(upcoming_stop);
        }

        Some(upcoming_stops_with_eta)
    } else {
        None
    }
}
