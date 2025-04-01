/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::WaypointInfo;
use crate::common::types::{
    Latitude, Longitude, Point, Route, RouteFeature, RouteGeoJSON, Stop, StopFeature,
};
use crate::tools::error::AppError;
use serde_json::from_str;
use std::fs;
use std::fs::File;
use std::io::Read;

#[allow(clippy::expect_used)]
pub fn read_route_data(config_path: &str) -> Vec<Route> {
    let geometries = fs::read_dir(config_path).expect("Failed to read config path");
    let mut routes: Vec<Route> = vec![];

    for entry in geometries {
        let entry = entry.expect("Failed to read entry");
        let file_name = entry.file_name().to_string_lossy().to_string();

        let file_path = config_path.to_owned() + "/" + &file_name;
        let mut file = File::open(file_path).expect("Failed to open file");
        let mut contents = String::new();
        file.read_to_string(&mut contents).expect("Failed to read");

        let route = parse_route_geojson(&contents);
        routes.push(route);
    }

    routes
}

#[allow(clippy::expect_used)]
fn parse_route_geojson(geojson_str: &str) -> Route {
    let route_geojson: RouteGeoJSON = from_str(geojson_str).expect("Failed to parse route geojson");

    let mut route_feature: Option<RouteFeature> = None;
    let mut stops: Vec<Stop> = vec![];

    for feature in route_geojson.features {
        if feature["type"] == "Feature" {
            if feature["geometry"]["type"] == "LineString" {
                route_feature = Some(serde_json::from_value(feature).expect("REASON"));
            } else if feature["geometry"]["type"] == "Point" {
                let stop_feature: StopFeature = serde_json::from_value(feature).expect("REASON");
                stops.push(Stop {
                    name: stop_feature.properties.stop_name,
                    coordinates: stop_feature.geometry.coordinates,
                    is_intermediate_stop: true,
                });
            }
        }
    }

    let route_feature = route_feature.expect("Failed to parse route feature");

    // Creation of WaypointInfo from the Route Coordinates
    let mut waypoints: Vec<WaypointInfo> = route_feature
        .geometry
        .coordinates
        .iter()
        .map(|coord| WaypointInfo {
            coordinate: coord.clone(),
            stop: None,
            upcoming_stop: None,
        })
        .collect();

    // Project stops onto route coordinates and put them in the waypoint vector
    for stop in &stops {
        let stop_point = Point {
            lat: Latitude(stop.coordinates[1]),
            lon: Longitude(stop.coordinates[0]),
        };

        if let Some(projection) = find_closest_point_on_route(&stop_point, &waypoints) {
            // The projection gives us the coordinate of the projected point on the route
            // we need to insert that point between the index and the index + 1 (here index is the projection index)
            // Here we would also add the stop info to the waypoint
            waypoints.insert(
                projection.segment_index as usize + 1,
                WaypointInfo {
                    coordinate: vec![
                        projection.projection_coords.lon.0,
                        projection.projection_coords.lat.0,
                    ],
                    stop: Some(stop.clone()),
                    upcoming_stop: None,
                },
            );
        } else {
            // add error here maybe
        }
    }

    // Iterate backwards in the waypoint to update the upcoming stop
    let mut upcoming_stop: Option<Stop> = None;
    for i in (0..waypoints.len()).rev() {
        if waypoints[i].stop.is_some() {
            upcoming_stop = waypoints[i].stop.clone();
            waypoints[i].upcoming_stop = upcoming_stop.clone();
        } else {
            waypoints[i].upcoming_stop = upcoming_stop.clone();
        }
    }

    Route {
        route_code: route_feature.properties.route_code,
        travel_mode: route_feature.properties.travel_mode,
        coordinates: waypoints,
        stops,
    }
}

#[derive(Debug)]
struct ProjectionPoint {
    segment_index: i32,
    distance: f64,
    projection_coords: Point,
}

fn calculate_perpendicular_distance(
    point: &Point,
    line_start: &[f64],
    line_end: &[f64],
) -> Option<(f64, Point)> {
    let x = point.lon.0;
    let y = point.lat.0;
    let x1 = line_start[0];
    let y1 = line_start[1];
    let x2 = line_end[0];
    let y2 = line_end[1];

    let line_length = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();

    let dot_product = ((x - x1) * (x2 - x1) + (y - y1) * (y2 - y1)) / (line_length * line_length);

    if (0.0..=1.0).contains(&dot_product) {
        let px = x1 + dot_product * (x2 - x1);
        let py = y1 + dot_product * (y2 - y1);

        let distance = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
        let projection_point = Point {
            lat: Latitude(py),
            lon: Longitude(px),
        };

        Some((distance, projection_point))
    } else {
        None
    }
}

fn find_closest_point_on_route(
    point: &Point,
    route_coordinates: &[WaypointInfo],
) -> Option<ProjectionPoint> {
    let mut closest_segment = ProjectionPoint {
        segment_index: -1,
        distance: f64::MAX,
        projection_coords: Point {
            lat: Latitude(0.0),
            lon: Longitude(0.0),
        },
    };

    for i in 0..route_coordinates.len() - 1 {
        if let Some((distance, projection_point)) = calculate_perpendicular_distance(
            point,
            &route_coordinates[i].coordinate,
            &route_coordinates[i + 1].coordinate,
        ) {
            if distance < closest_segment.distance {
                closest_segment = ProjectionPoint {
                    segment_index: i as i32,
                    distance,
                    projection_coords: projection_point,
                };
            }
        }
    }

    // TODO add config to check if closest segment distance is within a threshold
    let threshold = 100.0;
    if closest_segment.distance == f64::MAX
        || closest_segment.segment_index == -1
        || closest_segment.distance > threshold
    {
        None
    } else {
        Some(closest_segment)
    }
}

pub fn get_next_stop_by_route_code(
    routes: &[Route],
    route_code: &str,
    point: &Point,
) -> Result<Option<Stop>, AppError> {
    let route = routes
        .iter()
        .find(|r| r.route_code == route_code)
        .ok_or_else(|| AppError::InternalError("Route not found".to_string()))?;

    if let Some(projection) = find_closest_point_on_route(point, &route.coordinates) {
        // from the returned index we will iterate backwards and find the first stop that is within the radius
        // and using that point we will find the stop field present in the waypoint
        Ok(find_first_stop_within_radius(projection, route))
    } else {
        Ok(None)
    }
}

fn find_first_stop_within_radius(projection: ProjectionPoint, route: &Route) -> Option<Stop> {
    // check if the projection idx distance is less than the threshold distance,
    // then choose its upcoming stop as the stop
    // else choose the the upcoming stop for ind + 1

    let threshold = 100.0;
    if projection.distance < threshold {
        Some(
            route.coordinates[projection.segment_index as usize]
                .upcoming_stop
                .clone()?,
        )
    } else {
        Some(
            route.coordinates[projection.segment_index as usize + 1]
                .upcoming_stop
                .clone()?,
        )
    }
}
