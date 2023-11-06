/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{web, App, HttpServer};
use location_tracking_service::{
    common::types::*,
    domain::api,
    drainer::run_drainer,
    environment::{AppConfig, AppState},
    middleware::*,
};
use shared::utils::logger::*;
use shared::{tools::error::AppError, utils::prometheus::prometheus_metrics};
use std::{
    env::var,
    sync::atomic::{AtomicBool, Ordering},
};
use std::{net::Ipv4Addr, sync::Arc};
use tokio::signal::unix::SignalKind;
use tokio::{
    signal::unix::signal,
    sync::mpsc::{self, Receiver, Sender},
};
use tracing_actix_web::TracingLogger;

pub fn read_dhall_config(config_path: &str) -> Result<AppConfig, String> {
    let config = serde_dhall::from_file(config_path).parse::<AppConfig>();
    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(format!("Error reading config: {}", e)),
    }
}

#[actix_web::main]
async fn start_server() -> std::io::Result<()> {
    let dhall_config_path = var("DHALL_CONFIG")
        .unwrap_or_else(|_| "./dhall-configs/dev/location_tracking_service.dhall".to_string());
    let app_config = read_dhall_config(&dhall_config_path).unwrap_or_else(|err| {
        println!("Dhall Config Reading Error : {}", err);
        std::process::exit(1);
    });

    let _guard = setup_tracing(app_config.logger_cfg);

    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .unwrap_or(&"Unknown panic");
        error!("Panic Occured : {payload}");
    }));

    let port = app_config.port;
    let workers = app_config.workers;
    let max_allowed_req_size = app_config.max_allowed_req_size;

    #[allow(clippy::type_complexity)]
    let (sender, receiver): (
        Sender<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
        Receiver<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
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

    let prometheus = prometheus_metrics();

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .app_data(
                web::JsonConfig::default()
                    .limit(max_allowed_req_size)
                    .error_handler(|err, _| AppError::UnprocessibleRequest(err.to_string()).into()),
            )
            .app_data(web::PayloadConfig::default().limit(max_allowed_req_size))
            .wrap(LogIncomingRequestBody)
            .wrap(RequestTimeout)
            .wrap(CheckContentLength)
            .wrap(IncomingRequestMetrics)
            .wrap(TracingLogger::<DomainRootSpanBuilder>::new())
            .wrap(prometheus.clone())
            .configure(api::handler)
    })
    .workers(workers)
    .bind((Ipv4Addr::UNSPECIFIED, port))?
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
