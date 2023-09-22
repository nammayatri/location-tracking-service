/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{web, App, HttpServer};
use location_tracking_service::{
    common::{types::*, utils::get_current_bucket},
    domain::api,
    environment::{AppConfig, AppState},
    middleware::*,
    redis::commands::*,
};
use shared::redis::types::RedisConnectionPool;
use shared::utils::{
    logger::*,
    prometheus::{self, *},
};
use std::collections::HashMap;
use std::env::var;
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::interval;
use tracing_actix_web::TracingLogger;

pub fn read_dhall_config(config_path: &str) -> Result<AppConfig, String> {
    let config = serde_dhall::from_file(config_path).parse::<AppConfig>();
    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(format!("Error reading config: {}", e)),
    }
}

async fn drain_driver_locations(
    driver_locations: &Vec<(Dimensions, Latitude, Longitude, DriverId)>,
    bucket_size: u64,
    near_by_bucket_threshold: u64,
    non_persistent_redis: &RedisConnectionPool,
) {
    info!(tag = "[Queued Entries For Draining]", length = %driver_locations.len(), "Queue: {:?}\nPushing to redis server", driver_locations);

    let mut queue: HashMap<Dimensions, Vec<(Latitude, Longitude, DriverId)>> = HashMap::new();
    for (dimensions, lat, lon, driver_id) in driver_locations.iter() {
        queue
            .entry(dimensions.clone())
            .or_insert_with(Vec::new)
            .push((*lat, *lon, driver_id.clone()));
    }

    let bucket = get_current_bucket(&bucket_size);

    if let Ok(bucket) = bucket {
        for (dimensions, geo_entries) in queue.iter() {
            let merchant_id = &dimensions.merchant_id;
            let city = &dimensions.city;
            let vehicle_type = &dimensions.vehicle_type;

            if !geo_entries.is_empty() {
                let res = push_drainer_driver_location(
                    merchant_id,
                    city,
                    vehicle_type,
                    &bucket,
                    geo_entries,
                    &bucket_size,
                    &near_by_bucket_threshold,
                    non_persistent_redis,
                )
                .await;

                if let Err(err) = res {
                    error!(tag = "[Error Pushing To Redis]", error = %err);
                }
            }
        }
    }
}

async fn run_drainer(
    mut rx: mpsc::Receiver<(Dimensions, Latitude, Longitude, DriverId)>,
    drainer_delay: u64,
    drainer_size: usize,
    bucket_size: u64,
    near_by_bucket_threshold: u64,
    non_persistent_redis: &RedisConnectionPool,
) {
    let mut driver_locations = Vec::new();
    let mut timer = interval(Duration::from_secs(drainer_delay));
    loop {
        tokio::select! {
            item = rx.recv() => {
                info!(tag = "[Recieved Entries For Queuing]", length = %(driver_locations.len() + 1));
                match item {
                    Some(item) => {
                        prometheus::QUEUE_GUAGE.inc();
                        driver_locations.push((item.0, item.1, item.2, item.3));

                        if driver_locations.len() > (0.5 * drainer_size as f32) as usize {
                            drain_driver_locations(&driver_locations, bucket_size, near_by_bucket_threshold, non_persistent_redis).await;
                            for _ in 0..driver_locations.len() {
                                prometheus::QUEUE_GUAGE.dec();
                            }
                            driver_locations.clear();
                        }
                    },
                    None => break,
                }
            },
            _ = timer.tick() => {
                info!(tag = "[Checking Queue]", length = %driver_locations.len());
                if !driver_locations.is_empty() {
                    drain_driver_locations(&driver_locations, bucket_size, near_by_bucket_threshold, non_persistent_redis).await;
                    for _ in 0..driver_locations.len() {
                        prometheus::QUEUE_GUAGE.dec();
                    }
                    driver_locations.clear();
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

    #[allow(clippy::type_complexity)]
    let (sender, receiver): (
        Sender<(Dimensions, Latitude, Longitude, DriverId)>,
        Receiver<(Dimensions, Latitude, Longitude, DriverId)>,
    ) = mpsc::channel(app_config.drainer_size);

    let app_state = AppState::new(app_config, sender).await;

    let data = web::Data::new(app_state);

    let thread_data = data.clone();
    let channel_thread = tokio::spawn(async move {
        run_drainer(
            receiver,
            thread_data.drainer_delay,
            thread_data.drainer_size,
            thread_data.bucket_size,
            thread_data.nearby_bucket_threshold,
            &thread_data.non_persistent_redis,
        )
        .await;
    });

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(IncomingRequestMetrics)
            .wrap(TracingLogger::<DomainRootSpanBuilder>::new())
            .wrap(prometheus_metrics())
            .configure(api::handler)
    })
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
