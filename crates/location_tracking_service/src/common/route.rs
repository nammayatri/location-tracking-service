/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::common::types::{
    Latitude, Longitude, Point, Route, RouteFeature, RouteGeoJSON, Stop, StopFeature,
};
use crate::common::utils::*;
use crate::outbound::external::compute_routes;
use crate::redis::commands::{cache_google_stop_duration, get_google_stop_duration};
use reqwest::Url;
use rustc_hash::FxHashMap;
use serde_json::from_str;
use shared::redis::types::RedisConnectionPool;
use shared::tools::aws::get_files_in_directory_from_s3;
use std::cmp::max;
use std::env::var;
use std::fs;
use std::fs::File;
use std::io::Read;

#[allow(clippy::expect_used)]
pub async fn read_route_data(
    redis: &RedisConnectionPool,
    config_bucket: &str,
    config_prefix: &str,
    google_compute_route_url: &Url,
    google_api_key: &str,
) -> FxHashMap<String, Route> {
    if var("DEV").is_ok() {
        let config_path =
            var("ROUTE_GEO_JSON_CONFIG").unwrap_or_else(|_| "./route_geo_json_config".to_string());
        let geometries = fs::read_dir(&config_path).expect("Failed to read config path");
        let mut routes: FxHashMap<String, Route> = FxHashMap::default();

        for entry in geometries {
            let entry = entry.expect("Failed to read entry");
            let file_name = entry.file_name().to_string_lossy().to_string();

            let file_path = config_path.to_owned() + "/" + &file_name;
            let mut file = File::open(file_path).expect("Failed to open file");
            let mut contents = String::new();
            file.read_to_string(&mut contents).expect("Failed to read");

            let route =
                parse_route_geojson(redis, &contents, google_compute_route_url, google_api_key)
                    .await;
            routes.insert(route.route_code.clone(), route);
        }

        routes
    } else {
        let geometries = get_files_in_directory_from_s3(config_bucket, config_prefix)
            .await
            .expect("Failed to fetch files from S3");

        let mut routes: FxHashMap<String, Route> = FxHashMap::default();
        for (_, data) in geometries {
            let data = String::from_utf8(data).expect("Failed to convert to string");
            let route =
                parse_route_geojson(redis, &data, google_compute_route_url, google_api_key).await;
            routes.insert(route.route_code.clone(), route);
        }

        routes
    }
}

#[allow(clippy::expect_used)]
async fn parse_route_geojson(
    redis: &RedisConnectionPool,
    geojson_str: &str,
    google_compute_route_url: &Url,
    google_api_key: &str,
) -> Route {
    let route_geojson: RouteGeoJSON = from_str(geojson_str).expect("Failed to parse route geojson");

    let mut route_feature: Option<RouteFeature> = None;

    let mut stops: Vec<(String, String, Point, StopType)> = vec![];

    for feature in route_geojson.features {
        if feature["type"] == "Feature" {
            if feature["geometry"]["type"] == "LineString" {
                route_feature = Some(serde_json::from_value(feature).expect("REASON"));
            } else if feature["geometry"]["type"] == "Point" {
                let stop_feature: StopFeature = serde_json::from_value(feature).expect("REASON");
                let stop_lat = stop_feature.geometry.coordinates.get(1).expect("REASON");
                let stop_lon = stop_feature.geometry.coordinates.first().expect("REASON");
                stops.push((
                    stop_feature.properties.stop_name.to_owned(),
                    stop_feature.properties.stop_code.to_owned(),
                    Point {
                        lat: Latitude(*stop_lat),
                        lon: Longitude(*stop_lon),
                    },
                    if stop_feature.properties.stop_name == "ROUTE CORRECTION" {
                        StopType::RouteCorrectionStop
                    } else {
                        StopType::IntermediateStop
                    },
                ));
            }
        }
    }

    let route_feature = route_feature.expect("Failed to parse route feature");

    // Creation of WaypointInfo from the Route Coordinates
    let mut waypoints: Vec<(Point, Option<Stop>)> = route_feature
        .geometry
        .coordinates
        .into_iter()
        .map(|coord| {
            let coord_lat = coord.get(1).expect("REASON");
            let coord_lon = coord.first().expect("REASON");
            (
                Point {
                    lat: Latitude(*coord_lat),
                    lon: Longitude(*coord_lon),
                },
                None,
            )
        })
        .collect();

    // Project stops onto route coordinates and put them in the waypoint vector
    for (i, (stop_name, stop_code, stop_point, stop_type)) in stops.iter().enumerate() {
        let projection = find_closest_point_on_route(
            stop_point,
            waypoints.iter().map(|(pt, _)| pt.to_owned()).collect(),
        )
        .expect("Failed to find closest point on route");

        // The projection gives us the coordinate of the projected point on the route
        // we need to insert that point between the index and the index + 1 (here index is the projection index)
        // Here we would also add the stop info to the waypoint
        waypoints.insert(
            projection.segment_index as usize + 1,
            (
                projection.projection_point,
                Some(Stop {
                    name: stop_name.to_owned(),
                    stop_code: stop_code.to_owned(),
                    coordinate: stop_point.to_owned(),
                    distance_to_upcoming_intermediate_stop: Meters(0),
                    duration_to_upcoming_intermediate_stop: Seconds(0),
                    distance_from_previous_intermediate_stop: Meters(0),
                    stop_type: stop_type.to_owned(),
                    stop_idx: i,
                }),
            ),
        );
    }

    let mut waypoints: Vec<(Point, Option<Stop>)> = waypoints
        .into_iter()
        .filter_map(|(coordinate, stop)| {
            if stop.is_none() {
                if stops.iter().any(|(_, _, stop_coordinate, _)| {
                    distance_between_in_meters(stop_coordinate, &coordinate) as u64 == 0
                }) {
                    None
                } else {
                    Some((coordinate, stop))
                }
            } else {
                Some((coordinate, stop))
            }
        })
        .collect();

    // Iterate backwards in the waypoint to update the upcoming stop
    let mut upcoming_stop_total_distance_duration = None;
    let mut upcoming_intermediate_stop: Option<Stop> = None;
    for i in (0..waypoints.len()).rev() {
        match (
            waypoints[i].to_owned(),
            upcoming_intermediate_stop.to_owned(),
        ) {
            (
                (
                    waypoint_coordinate,
                    Some(Stop {
                        name,
                        stop_code,
                        coordinate,
                        stop_type,
                        stop_idx,
                        ..
                    }),
                ),
                upcoming_stop,
            ) => {
                if stop_type == StopType::IntermediateStop {
                    upcoming_stop_total_distance_duration = Some(
                        compute_distance_and_duration_to_upcoming_intermediate_stop(
                            redis,
                            &waypoints,
                            i,
                            stop_code.to_owned(),
                            &coordinate,
                            google_compute_route_url,
                            google_api_key,
                        )
                        .await,
                    );

                    let distance_from_previous_intermediate_stop =
                        upcoming_stop_total_distance_duration
                            .map(|(Meters(total_distance), _)| Meters(max(0, total_distance)))
                            .unwrap_or(Meters(0));

                    upcoming_intermediate_stop = Some(Stop {
                        name: name.to_owned(),
                        stop_code: stop_code.to_owned(),
                        coordinate: coordinate.to_owned(),
                        distance_to_upcoming_intermediate_stop: Meters(0),
                        duration_to_upcoming_intermediate_stop: Seconds(0),
                        distance_from_previous_intermediate_stop,
                        stop_type,
                        stop_idx,
                    });

                    waypoints[i] = (
                        waypoint_coordinate,
                        Some(Stop {
                            name: name.to_owned(),
                            stop_code: stop_code.to_owned(),
                            coordinate: coordinate.to_owned(),
                            distance_to_upcoming_intermediate_stop: Meters(0),
                            duration_to_upcoming_intermediate_stop: Seconds(0),
                            distance_from_previous_intermediate_stop,
                            stop_type: StopType::IntermediateStop,
                            stop_idx,
                        }),
                    );
                } else if stop_type == StopType::RouteCorrectionStop {
                    if let Some(Stop {
                        name,
                        stop_code,
                        coordinate,
                        distance_to_upcoming_intermediate_stop:
                            Meters(distance_to_upcoming_intermediate_stop),
                        duration_to_upcoming_intermediate_stop:
                            Seconds(duration_to_upcoming_intermediate_stop),
                        stop_type,
                        stop_idx,
                        ..
                    }) = upcoming_stop
                    {
                        let (segment_distance, segment_duration) =
                            get_segment_distance_duration_to_upcoming_intermediate_stop(
                                &waypoint_coordinate,
                                &waypoints,
                                i,
                                &upcoming_stop_total_distance_duration,
                            );

                        let distance_to_upcoming_intermediate_stop = Meters(
                            distance_to_upcoming_intermediate_stop + segment_distance.inner(),
                        );
                        let duration_to_upcoming_intermediate_stop = Seconds(
                            duration_to_upcoming_intermediate_stop + segment_duration.inner(),
                        );
                        let distance_from_previous_intermediate_stop =
                            upcoming_stop_total_distance_duration
                                .map(|(Meters(total_distance), _)| {
                                    Meters(max(
                                        0,
                                        total_distance
                                            - distance_to_upcoming_intermediate_stop.inner(),
                                    ))
                                })
                                .unwrap_or(Meters(0));

                        upcoming_intermediate_stop = Some(Stop {
                            name: name.to_owned(),
                            stop_code: stop_code.to_owned(),
                            coordinate: coordinate.to_owned(),
                            distance_to_upcoming_intermediate_stop,
                            duration_to_upcoming_intermediate_stop,
                            distance_from_previous_intermediate_stop,
                            stop_type: stop_type.to_owned(),
                            stop_idx,
                        });

                        waypoints[i] = (
                            waypoint_coordinate,
                            Some(Stop {
                                name: name.to_owned(),
                                stop_code: stop_code.to_owned(),
                                coordinate: coordinate.to_owned(),
                                distance_to_upcoming_intermediate_stop,
                                duration_to_upcoming_intermediate_stop,
                                distance_from_previous_intermediate_stop,
                                stop_type: StopType::RouteCorrectionStop,
                                stop_idx,
                            }),
                        );
                    }
                }
            }
            (
                (waypoint_coordinate, None),
                Some(Stop {
                    name,
                    stop_code,
                    coordinate,
                    distance_to_upcoming_intermediate_stop:
                        Meters(distance_to_upcoming_intermediate_stop),
                    duration_to_upcoming_intermediate_stop:
                        Seconds(duration_to_upcoming_intermediate_stop),
                    stop_type,
                    stop_idx,
                    ..
                }),
            ) => {
                let (segment_distance, segment_duration) =
                    get_segment_distance_duration_to_upcoming_intermediate_stop(
                        &waypoint_coordinate,
                        &waypoints,
                        i,
                        &upcoming_stop_total_distance_duration,
                    );

                println!(
                    "[parse_route_geojson] upcoming_stop_code: {}, segment_distance: {}",
                    stop_code,
                    segment_distance.inner()
                );

                let distance_to_upcoming_intermediate_stop =
                    Meters(distance_to_upcoming_intermediate_stop + segment_distance.inner());
                let duration_to_upcoming_intermediate_stop =
                    Seconds(duration_to_upcoming_intermediate_stop + segment_duration.inner());
                let distance_from_previous_intermediate_stop =
                    upcoming_stop_total_distance_duration
                        .map(|(Meters(total_distance), _)| {
                            Meters(max(
                                0,
                                total_distance as i32
                                    - distance_to_upcoming_intermediate_stop.inner() as i32,
                            ) as u32)
                        })
                        .unwrap_or(Meters(0));

                upcoming_intermediate_stop = Some(Stop {
                    name: name.to_owned(),
                    stop_code: stop_code.to_owned(),
                    coordinate: coordinate.to_owned(),
                    distance_to_upcoming_intermediate_stop,
                    duration_to_upcoming_intermediate_stop,
                    distance_from_previous_intermediate_stop,
                    stop_type: stop_type.to_owned(),
                    stop_idx,
                });
                waypoints[i] = (
                    waypoint_coordinate,
                    Some(Stop {
                        name: name.to_owned(),
                        stop_code: stop_code.to_owned(),
                        coordinate: coordinate.to_owned(),
                        distance_to_upcoming_intermediate_stop,
                        duration_to_upcoming_intermediate_stop,
                        distance_from_previous_intermediate_stop,
                        stop_type: StopType::UpcomingStop,
                        stop_idx,
                    }),
                )
            }
            (_, None) => {}
        }
    }

    let waypoints_info = waypoints
        .into_iter()
        .filter_map(|(pt, stop)| {
            stop.map(|stop| WaypointInfo {
                coordinate: pt,
                stop,
            })
        })
        .collect();

    Route {
        route_code: route_feature.properties.route_code,
        travel_mode: route_feature.properties.travel_mode,
        waypoints: waypoints_info,
    }
}

fn get_segment_distance_duration_to_upcoming_intermediate_stop(
    curr_waypoint_coordinate: &Point,
    waypoints: &[(Point, Option<Stop>)],
    i: usize,
    upcoming_stop_total_distance_duration: &Option<(Meters, Seconds)>,
) -> (Meters, Seconds) {
    let segment_distance = Meters(
        waypoints
            .get(i + 1)
            .map(|(next_waypoint_coordinate, _)| {
                distance_between_in_meters(curr_waypoint_coordinate, next_waypoint_coordinate)
                    as u32
            })
            .unwrap_or(0),
    );

    // 1000m -> 1 min
    // 100m -> x min
    // x = (100 * 1) / 1000 = 0.1 min
    let segment_duration = Seconds(
        upcoming_stop_total_distance_duration
            .map(|(Meters(total_distance), Seconds(total_duration))| {
                (segment_distance.inner() * total_duration) / max(1, total_distance)
            })
            .unwrap_or(0),
    );

    (segment_distance, segment_duration)
}

#[allow(clippy::too_many_arguments)]
async fn compute_distance_and_duration_to_upcoming_intermediate_stop(
    redis: &RedisConnectionPool,
    waypoints: &[(Point, Option<Stop>)],
    i: usize,
    destination_intermediate_stop_code: String,
    destination_intermediate_stop_coordinate: &Point,
    google_compute_route_url: &Url,
    google_api_key: &str,
) -> (Meters, Seconds) {
    let (origin_intermediate_stop_distance, origin_intermediate_stops, _) = waypoints.iter().take(i).rev().fold(
        (
            waypoints
                .get(i)
                .map(|(coordinate, _)| (coordinate.to_owned(), 0)),
            vec![(destination_intermediate_stop_coordinate.to_owned(), destination_intermediate_stop_code.to_owned())],
            false
        ),
        |(acc_distance, mut acc_stop, is_intermediate_stop_reached), (coordinate, stop)| {
            if is_intermediate_stop_reached {
                (acc_distance, acc_stop, is_intermediate_stop_reached)
            } else {
                let distance = if let Some((coordinate_prev_stop, distance_prev_stop)) =
                    acc_distance.as_ref()
                {
                    let distance = distance_prev_stop
                        + distance_between_in_meters(coordinate, coordinate_prev_stop) as u32;
                    println!(
                        "[compute_distance_and_duration_to_upcoming_intermediate_stop] upcoming_stop_code: {}, segment_distance: {}",
                        destination_intermediate_stop_code,
                       distance_between_in_meters(coordinate, coordinate_prev_stop) as u32
                    );
                    Some((coordinate.to_owned(), distance))
                } else {
                    Some((coordinate.to_owned(), 0))
                };
                if let Some(stop) = stop {
                    if stop.stop_type == StopType::IntermediateStop {
                        acc_stop.push((coordinate.to_owned(), stop.stop_code.to_owned()));
                        (distance, acc_stop, true)
                    } else if stop.stop_type == StopType::RouteCorrectionStop {
                        acc_stop.push((coordinate.to_owned(), stop.stop_code.to_owned()));
                        (distance, acc_stop, false)
                    } else {
                        (distance, acc_stop, false)
                    }
                } else {
                    (distance, acc_stop, false)
                }
            }
        },
    );

    if let Some((_, distance)) = origin_intermediate_stop_distance {
        let mut total_duration = Seconds(0);
        let mut stops_iter = origin_intermediate_stops.iter().rev();

        while let (Some((origin_coordinate, origin_code)), Some((dest_coordinate, dest_code))) =
            (stops_iter.next(), stops_iter.next())
        {
            let segment_duration = if let Ok(Some(duration)) =
                get_google_stop_duration(redis, origin_code.to_owned(), dest_code.to_owned()).await
            {
                duration
            } else if let Ok(routes_response) = compute_routes(
                google_compute_route_url,
                google_api_key,
                origin_coordinate,
                dest_coordinate,
                vec![],
                TravelMode::Drive,
            )
            .await
            {
                if let Some(route) = routes_response.routes.first() {
                    if let Ok(duration) = route.duration.replace("s", "").parse::<u32>() {
                        let _ = cache_google_stop_duration(
                            redis,
                            origin_code.to_owned(),
                            dest_code.to_owned(),
                            Seconds(duration),
                        )
                        .await;
                        Seconds(duration)
                    } else {
                        Seconds(0)
                    }
                } else {
                    Seconds(0)
                }
            } else {
                Seconds(0)
            };

            total_duration = Seconds(total_duration.inner() + segment_duration.inner());
        }

        (Meters(distance), total_duration)
    } else {
        (Meters(0), Seconds(0))
    }
}
