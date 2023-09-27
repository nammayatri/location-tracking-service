/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{web, App, HttpServer};
use fred::types::{GeoPosition, GeoValue};
use location_tracking_service::{
    common::{types::*, utils::get_current_bucket},
    domain::api,
    environment::{AppConfig, AppState},
    middleware::*,
    redis::{commands::*, keys::driver_loc_bucket_key},
};
use rustc_hash::FxHashMap;
use shared::utils::{
    logger::*,
    prometheus::{self, *},
};
use shared::{redis::types::RedisConnectionPool, tools::error::AppError};
use std::{
    env::var,
    sync::atomic::{AtomicBool, Ordering},
};
use std::{sync::Arc, time::Duration};
use tokio::{
    signal::unix::signal,
    sync::mpsc::{self, Receiver, Sender},
};
use tokio::{signal::unix::SignalKind, time::interval};
use tracing_actix_web::TracingLogger;

pub fn read_dhall_config(config_path: &str) -> Result<AppConfig, String> {
    let config = serde_dhall::from_file(config_path).parse::<AppConfig>();
    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(format!("Error reading config: {}", e)),
    }
}

async fn drain_driver_locations(
    driver_locations: &FxHashMap<String, Vec<GeoValue>>,
    bucket_size: u64,
    near_by_bucket_threshold: u64,
    non_persistent_redis: &RedisConnectionPool,
) {
    info!(
        tag = "[Queued Entries For Draining]",
        "Queue: {:?}\nPushing to redis server", driver_locations
    );

    let res = push_drainer_driver_location(
        driver_locations,
        &bucket_size,
        &near_by_bucket_threshold,
        non_persistent_redis,
    )
    .await;

    if let Err(err) = res {
        error!(tag = "[Error Pushing To Redis]", error = %err);
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_drainer(
    mut rx: mpsc::Receiver<(Dimensions, Latitude, Longitude, DriverId)>,
    graceful_termination_requested: Arc<AtomicBool>,
    drainer_capacity: usize,
    drainer_delay: u64,
    new_ride_drainer_delay: u64,
    bucket_size: u64,
    near_by_bucket_threshold: u64,
    non_persistent_redis: &RedisConnectionPool,
) {
    let mut driver_locations: FxHashMap<String, Vec<GeoValue>> = FxHashMap::default();
    let mut timer = interval(Duration::from_secs(drainer_delay));

    let mut new_ride_driver_locations: FxHashMap<String, Vec<GeoValue>> = FxHashMap::default();
    let mut new_ride_timer = interval(Duration::from_secs(new_ride_drainer_delay));

    let mut drainer_size = 0;
    let mut new_ride_drainer_size = 0;
    loop {
        if graceful_termination_requested.load(Ordering::Relaxed) {
            info!(tag = "[Graceful Shutting Down]", length = %drainer_size);
            if !driver_locations.is_empty() {
                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                drain_driver_locations(
                    &driver_locations,
                    bucket_size,
                    near_by_bucket_threshold,
                    non_persistent_redis,
                )
                .await;
                prometheus::QUEUE_COUNTER.reset();
            }
            if !new_ride_driver_locations.is_empty() {
                info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                drain_driver_locations(
                    &new_ride_driver_locations,
                    bucket_size,
                    near_by_bucket_threshold,
                    non_persistent_redis,
                )
                .await;
                prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
            }
            break;
        }
        tokio::select! {
            item = rx.recv() => {
                info!(tag = "[Recieved Entries For Queuing]");
                match item {
                    Some((Dimensions { merchant_id, city, vehicle_type, new_ride }, Latitude(latitude), Longitude(longitude), DriverId(driver_id))) => {
                        if let Ok(bucket) = get_current_bucket(&bucket_size) {
                            if new_ride {
                                new_ride_driver_locations
                                    .entry(driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket))
                                    .or_insert_with(Vec::new)
                                    .push(GeoValue {
                                        coordinates: GeoPosition {
                                            latitude,
                                            longitude,
                                        },
                                        member: driver_id.into(),
                                    });
                                new_ride_drainer_size += 1;
                                prometheus::NEW_RIDE_QUEUE_COUNTER.inc();
                            } else {
                                driver_locations
                                    .entry(driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket))
                                    .or_insert_with(Vec::new)
                                    .push(GeoValue {
                                        coordinates: GeoPosition {
                                            latitude,
                                            longitude,
                                        },
                                        member: driver_id.into(),
                                    });
                                drainer_size += 1;
                                prometheus::QUEUE_COUNTER.inc();
                            }
                            if drainer_size >= drainer_capacity {
                                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                                drain_driver_locations(&driver_locations, bucket_size, near_by_bucket_threshold, non_persistent_redis).await;
                                // Cleanup
                                prometheus::QUEUE_COUNTER.reset();
                                drainer_size = 0;
                                driver_locations.clear();
                            }
                            if new_ride_drainer_size >= drainer_capacity {
                                info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                                drain_driver_locations(&new_ride_driver_locations, bucket_size, near_by_bucket_threshold, non_persistent_redis).await;
                                // Cleanup
                                prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
                                new_ride_drainer_size = 0;
                                new_ride_driver_locations.clear();
                            }
                        }
                    },
                    None => break,
                }
            },
            _ = timer.tick() => {
                if !driver_locations.is_empty() {
                    info!(tag = "[Draining Queue]", length = %drainer_size);
                    drain_driver_locations(&driver_locations, bucket_size, near_by_bucket_threshold, non_persistent_redis).await;
                    // Cleanup
                    prometheus::QUEUE_COUNTER.reset();
                    drainer_size = 0;
                    driver_locations.clear();
                }
            },
            _ = new_ride_timer.tick() => {
                if !new_ride_driver_locations.is_empty() {
                    info!(tag = "[Draining Queue - New Ride]", length = %new_ride_drainer_size);
                    drain_driver_locations(&new_ride_driver_locations, bucket_size, near_by_bucket_threshold, non_persistent_redis).await;
                    // Cleanup
                    prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
                    new_ride_drainer_size = 0;
                    new_ride_driver_locations.clear();
                }
            },
        }
    }
}

#[actix_web::main]
async fn start_server() -> std::io::Result<()> {
    let dhall_config_path = var("DHALL_CONFIG")
        .unwrap_or_else(|_| "./dhall_config/location_tracking_service.dhall".to_string());
    let app_config = read_dhall_config(&dhall_config_path).unwrap_or_else(|err| {
        println!("Dhall Config Reading Error : {}", err);
        std::process::exit(1);
    });

    let _guard = setup_tracing(app_config.logger_cfg);

    let port = app_config.port;
    let workers = app_config.workers;

    #[allow(clippy::type_complexity)]
    let (sender, receiver): (
        Sender<(Dimensions, Latitude, Longitude, DriverId)>,
        Receiver<(Dimensions, Latitude, Longitude, DriverId)>,
    ) = mpsc::channel(app_config.drainer_size);

    let app_state = AppState::new(app_config, sender).await;

    let data = web::Data::new(app_state);

    let graceful_termination_requested = Arc::new(AtomicBool::new(false));
    let graceful_termination_requested_sigterm = graceful_termination_requested.to_owned();
    let graceful_termination_requested_sigint = graceful_termination_requested.to_owned();
    // Listen for SIGTERM signal.
    tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        sigterm.recv().await;
        graceful_termination_requested_sigterm.store(true, Ordering::Relaxed);
    });
    // Listen for SIGINT (Ctrl+C) signal.
    tokio::spawn(async move {
        let mut ctrl_c = signal(SignalKind::interrupt()).unwrap();
        ctrl_c.recv().await;
        graceful_termination_requested_sigint.store(true, Ordering::Relaxed);
    });

    let (
        drainer_size,
        drainer_delay,
        new_ride_drainer_delay,
        bucket_size,
        nearby_bucket_threshold,
        non_persistent_redis,
    ) = (
        data.drainer_size,
        data.drainer_delay,
        data.new_ride_drainer_delay,
        data.bucket_size,
        data.nearby_bucket_threshold,
        data.non_persistent_redis.clone(),
    );
    let channel_thread = tokio::spawn(async move {
        run_drainer(
            receiver,
            graceful_termination_requested,
            drainer_size,
            drainer_delay,
            new_ride_drainer_delay,
            bucket_size,
            nearby_bucket_threshold,
            &non_persistent_redis,
        )
        .await;
    });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .app_data(
                web::JsonConfig::default()
                    .error_handler(|err, _| AppError::UnprocessibleRequest(err.to_string()).into()),
            )
            .wrap(IncomingRequestMetrics)
            .wrap(TracingLogger::<DomainRootSpanBuilder>::new())
            .wrap(prometheus_metrics())
            .configure(api::handler)
    })
    .workers(workers)
    .bind(("0.0.0.0", port))?
    .run()
    .await?;

    channel_thread
        .await
        .expect("Channel listener thread panicked");

    Ok(())
}

fn main() {
    start_server().expect("Failed to start the server");
}
