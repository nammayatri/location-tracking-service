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
use crate::environment::S3Config;
use crate::outbound::external::compute_routes;
use crate::redis::commands::{
    cache_google_stop_duration, get_google_stop_duration, with_lock_redis,
};
use crate::redis::keys::google_route_duration_cache_processing_key;
use crate::tools::error::AppError;
use chrono::{NaiveTime, Timelike};
use reqwest::Url;
use rustc_hash::FxHashMap;
use serde_json::from_str;
use shared::redis::types::RedisConnectionPool;
use shared::tools::aws::get_files_in_directory_from_s3;
use shared::{termination, tools::prometheus::TERMINATION};
use std::{cmp::max, env::var, fs, fs::File, io::Read, sync::Arc, time::Duration};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::RwLock;
use tokio::time::Instant;
use tracing::*;

pub async fn read_route_data(
    redis: &RedisConnectionPool,
    config_bucket: &str,
    config_prefix: &str,
    google_compute_route_url: &Url,
    google_api_key: &str,
    force_refresh: bool,
) -> Result<FxHashMap<String, Route>, AppError> {
    if var("DEV").is_ok() {
        let config_path =
            var("ROUTE_GEO_JSON_CONFIG").unwrap_or_else(|_| "./route_geo_json_config".to_string());
        let geometries = fs::read_dir(&config_path).map_err(|err| {
            AppError::InternalError(format!("Failed to read config path: {}", err))
        })?;
        let mut routes: FxHashMap<String, Route> = FxHashMap::default();

        for entry in geometries {
            let entry = entry
                .map_err(|err| AppError::InternalError(format!("Failed to read entry: {}", err)))?;
            let file_name = entry.file_name().to_string_lossy().to_string();

            let file_path = config_path.to_owned() + "/" + &file_name;
            let mut file = File::open(file_path)
                .map_err(|err| AppError::InternalError(format!("Failed to open file: {}", err)))?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .map_err(|err| AppError::InternalError(format!("Failed to read file: {}", err)))?;

            let route = parse_route_geojson(
                redis,
                &contents,
                google_compute_route_url,
                google_api_key,
                force_refresh,
            )
            .await?;
            routes.insert(route.route_code.clone(), route);
        }

        Ok(routes)
    } else {
        let geometries = get_files_in_directory_from_s3(config_bucket, config_prefix)
            .await
            .map_err(|err| {
                AppError::InternalError(format!("Failed to fetch files from S3: {}", err))
            })?;

        let mut routes: FxHashMap<String, Route> = FxHashMap::default();
        for (_, data) in geometries {
            let data = String::from_utf8(data).map_err(|err| {
                AppError::InternalError(format!("Failed to convert to string: {}", err))
            })?;
            let route = parse_route_geojson(
                redis,
                &data,
                google_compute_route_url,
                google_api_key,
                force_refresh,
            )
            .await?;
            routes.insert(route.route_code.clone(), route);
        }

        Ok(routes)
    }
}

async fn parse_route_geojson(
    redis: &RedisConnectionPool,
    geojson_str: &str,
    google_compute_route_url: &Url,
    google_api_key: &str,
    force_refresh: bool,
) -> Result<Route, AppError> {
    let route_geojson: RouteGeoJSON = from_str(geojson_str).map_err(|err| {
        AppError::InternalError(format!("Failed to parse route geojson: {}", err))
    })?;

    let mut route_feature: Option<RouteFeature> = None;

    let mut stops: Vec<(String, String, Point, StopType)> = vec![];

    for feature in route_geojson.features {
        if feature["type"] == "Feature" {
            if feature["geometry"]["type"] == "LineString" {
                route_feature = Some(serde_json::from_value(feature).map_err(|err| {
                    AppError::InternalError(format!("Failed to parse route feature: {}", err))
                })?);
            } else if feature["geometry"]["type"] == "Point" {
                let stop_feature: StopFeature = serde_json::from_value(feature).map_err(|err| {
                    AppError::InternalError(format!("Failed to parse stop feature: {}", err))
                })?;
                let stop_lat = stop_feature.geometry.coordinates.get(1).ok_or_else(|| {
                    AppError::InternalError("Failed to get stop latitude".to_string())
                })?;
                let stop_lon = stop_feature.geometry.coordinates.first().ok_or_else(|| {
                    AppError::InternalError("Failed to get stop longitude".to_string())
                })?;
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

    let route_feature = route_feature
        .ok_or_else(|| AppError::InternalError("Failed to find route feature".to_string()))?;

    // Creation of WaypointInfo from the Route Coordinates
    let mut waypoints: Vec<(Point, Option<Stop>)> = route_feature
        .geometry
        .coordinates
        .into_iter()
        .map(|coord| {
            let coord_lat = coord.get(1).ok_or_else(|| {
                AppError::InternalError("Failed to get coordinate latitude".to_string())
            })?;
            let coord_lon = coord.first().ok_or_else(|| {
                AppError::InternalError("Failed to get coordinate longitude".to_string())
            })?;
            Ok((
                Point {
                    lat: Latitude(*coord_lat),
                    lon: Longitude(*coord_lon),
                },
                None,
            ))
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    // Project stops onto route coordinates and put them in the waypoint vector
    for (i, (stop_name, stop_code, stop_point, stop_type)) in stops.iter().enumerate() {
        let projection = find_closest_point_on_route(
            stop_point,
            waypoints.iter().map(|(pt, _)| pt.to_owned()).collect(),
        )
        .ok_or_else(|| {
            AppError::InternalError("Failed to find closest point on route".to_string())
        })?;

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
                            force_refresh,
                        )
                        .await?,
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

    Ok(Route {
        route_code: route_feature.properties.route_code,
        travel_mode: route_feature.properties.travel_mode,
        waypoints: waypoints_info,
    })
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
    force_refresh: bool,
) -> Result<(Meters, Seconds), AppError> {
    let (origin_intermediate_stop_distance, origin_intermediate_stops, _) =
        waypoints.iter().take(i).rev().fold(
            (
                waypoints
                    .get(i)
                    .map(|(coordinate, _)| (coordinate.to_owned(), 0)),
                vec![(
                    destination_intermediate_stop_coordinate.to_owned(),
                    destination_intermediate_stop_code.to_owned(),
                )],
                false,
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
            let segment_duration = if let Ok(Some(duration)) = if !force_refresh {
                get_google_stop_duration(redis, origin_code.to_owned(), dest_code.to_owned()).await
            } else {
                Ok(None)
            } {
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

        Ok((Meters(distance), total_duration))
    } else {
        Ok((Meters(0), Seconds(0)))
    }
}

pub async fn start_route_refresh_task(
    redis: Arc<RedisConnectionPool>,
    routes: Arc<RwLock<FxHashMap<String, Route>>>,
    route_geo_json_config: S3Config,
    google_compute_route_url: Url,
    google_api_key: String,
    duration_cache_time_slots: Vec<NaiveTime>,
) -> Result<(), AppError> {
    let mut sorted_durations: Vec<_> = duration_cache_time_slots.iter().collect();
    sorted_durations.sort();

    let mut sigterm = signal(SignalKind::terminate()).map_err(|err| {
        AppError::InternalError(format!("Failed to create SIGTERM signal handler: {}", err))
    })?;
    let mut ctrl_c = signal(SignalKind::interrupt()).map_err(|err| {
        AppError::InternalError(format!("Failed to create SIGINT signal handler: {}", err))
    })?;

    let get_sleep = || {
        let now = chrono::Utc::now();
        let current_time_slot = now.time();
        if let Some(upcoming_time_slot) = sorted_durations
            .iter()
            .find(|&&slot| *slot > current_time_slot)
        {
            Some(tokio::time::sleep(Duration::from_secs(
                upcoming_time_slot.num_seconds_from_midnight() as u64
                    - current_time_slot.num_seconds_from_midnight() as u64,
            )))
        } else {
            sorted_durations.first().map(|upcoming_time_slot| {
                tokio::time::sleep(Duration::from_secs(
                    (86_400 + upcoming_time_slot.num_seconds_from_midnight() as u64)
                        - current_time_slot.num_seconds_from_midnight() as u64,
                ))
            })
        }
    };

    let mut sleep = get_sleep();
    let mut is_route_refresh_ongoing = false;
    let route_refresh_duration = Seconds(300);

    while let Some(current_sleep) = sleep {
        tokio::select! {
                _ = current_sleep => {
                if !is_route_refresh_ongoing {
                    match with_lock_redis(
                        &redis,
                        google_route_duration_cache_processing_key(),
                        300,
                        |args| async {
                            let (redis, config_bucket, config_prefix, google_compute_route_url, google_api_key) = args;

                            read_route_data(
                                &redis,
                                &config_bucket,
                                &config_prefix,
                                &google_compute_route_url,
                                &google_api_key,
                                true,
                            )
                            .await
                        },
                        (
                            redis.clone(),
                            route_geo_json_config.bucket.to_owned(),
                            route_geo_json_config.prefix.to_owned(),
                            google_compute_route_url.to_owned(),
                            google_api_key.to_owned()
                        )
                    )
                    .await {
                        Ok(routes_data) => {
                            is_route_refresh_ongoing = false;
                            sleep = get_sleep();
                            info!("[ROUTE_REFRESH_TASK_COMPLETED] : {:?}", routes_data);
                            for (route_code, route_data) in routes_data {
                                routes.write().await.insert(route_code.to_owned(), route_data.to_owned());
                            }
                        },
                        Err(err) => {
                            error!("[ROUTE_REFRESH_TASK_FAILED] : {:?}", err);
                            is_route_refresh_ongoing = true;
                            sleep = Some(tokio::time::sleep(Duration::from_secs(route_refresh_duration.inner() as u64)));
                        }
                    }
                } else {
                    is_route_refresh_ongoing = false;
                    sleep = get_sleep();
                    if let Ok(routes_data) = read_route_data(
                        &redis,
                        &route_geo_json_config.bucket,
                        &route_geo_json_config.prefix,
                        &google_compute_route_url,
                        &google_api_key,
                        false,
                    )
                    .await {
                        for (route_code, route_data) in routes_data {
                            routes.write().await.insert(route_code.to_owned(), route_data.to_owned());
                        }
                    }
                }
            },
            _ = sigterm.recv() => {
                termination!("graceful-termination", Instant::now());
                break;
            },
            _ = ctrl_c.recv() => {
                termination!("graceful-termination", Instant::now());
                break;
            }
        }
    }

    Ok(())
}
