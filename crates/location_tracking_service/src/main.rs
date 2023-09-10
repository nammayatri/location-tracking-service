/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{web, App, HttpServer};
use location_tracking_service::common::utils::get_current_bucket;
use location_tracking_service::domain::api;
use location_tracking_service::environment::{AppConfig, AppState};
use location_tracking_service::middleware::*;
use location_tracking_service::redis::commands::*;
use shared::tools::error::AppError;
use shared::utils::{logger::*, prometheus::*};
use std::env::var;
use tokio::{spawn, time::Duration};
use tracing_actix_web::TracingLogger;

pub fn read_dhall_config(config_path: &str) -> Result<AppConfig, String> {
    let config = serde_dhall::from_file(config_path).parse::<AppConfig>();
    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(format!("Error reading config: {}", e)),
    }
}

async fn run_drainer(data: web::Data<AppState>) -> Result<(), AppError> {
    let bucket = get_current_bucket(data.bucket_size)?;

    let queue = data.get_and_clear_queue().await;

    for (dimensions, geo_entries) in queue.iter() {
        let merchant_id = &dimensions.merchant_id;
        let city = &dimensions.city;
        let vehicle_type = &dimensions.vehicle_type;

        if !geo_entries.is_empty() {
            let _ = push_drainer_driver_location(
                data.clone(),
                merchant_id,
                city,
                vehicle_type,
                &bucket,
                geo_entries,
            )
            .await;
            info!(tag = "[Queued Entries For Draining]", length = %geo_entries.len(), "Queue: {:?}\nPushing to redis server", geo_entries);
        }
    }

    Ok(())
}

#[actix_web::main]
async fn start_server() -> std::io::Result<()> {
    setup_tracing();

    let dhall_config_path = var("DHALL_CONFIG")
        .unwrap_or_else(|_| "./dhall_config/location_tracking_service.dhall".to_string());
    let app_config = read_dhall_config(&dhall_config_path).unwrap_or_else(|err| {
        error!("Dhall Config Reading Error : {}", err);
        std::process::exit(1);
    });

    let port = app_config.port;

    let app_state = AppState::new(app_config).await;

    let data = web::Data::new(app_state);

    let thread_data = data.clone();
    spawn(async move {
        loop {
            info!(tag = "[Drainer]", "Draining From Queue to Redis...");
            let _ =
                tokio::time::sleep(Duration::from_secs(thread_data.clone().drainer_delay)).await;
            let _ = run_drainer(thread_data.clone()).await;
        }
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
    .await
}

fn main() {
    start_server().expect("Failed to start the server");
}
