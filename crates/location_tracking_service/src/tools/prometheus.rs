/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use actix_web_prom::PrometheusMetrics;
use prometheus::{opts, register_histogram_vec, register_int_counter, HistogramVec, IntCounter};
pub use shared::tools::prometheus::*;

pub static INCOMING_API: once_cell::sync::Lazy<HistogramVec> = once_cell::sync::Lazy::new(|| {
    register_histogram_vec!(
        opts!("http_request_duration_seconds", "Incoming API requests").into(),
        &["method", "handler", "status_code", "code", "version"]
    )
    .expect("Failed to register incoming API metrics")
});

pub static QUEUE_DRAINER_LATENCY: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            opts!("queue_drainer_latency", "Queue Drainer Montitoring").into(),
            &[]
        )
        .expect("Failed to register queue drainer latency metrics")
    });

pub static TOTAL_LOCATION_UPDATES: once_cell::sync::Lazy<IntCounter> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter!("total_location_updates", "Total Location Updates")
            .expect("Failed to register total location updates metrics")
    });

pub static TERMINATION: once_cell::sync::Lazy<HistogramVec> = once_cell::sync::Lazy::new(|| {
    register_histogram_vec!(
        opts!("termination", "Terminations").into(),
        &["type", "version"]
    )
    .expect("Failed to register termination metrics")
});

/// Macro that observes the duration of incoming API requests and logs metrics related to the request.
///
/// This macro captures key parameters of an incoming request like method, endpoint, status, code, and the time taken to process the request.
/// It then updates the `INCOMING_API` histogram with these metrics.
///
/// # Arguments
///
/// * `$method` - The HTTP method of the request (e.g., GET, POST).
/// * `$endpoint` - The endpoint or route of the request.
/// * `$status` - The HTTP status code of the response.
/// * `$code` - A specific code detailing more about the response, if available.
/// * `$start` - The time when the request was received. This is used to calculate the request duration.
#[macro_export]
macro_rules! incoming_api {
    ($method:expr, $endpoint:expr, $status:expr, $code:expr, $start:expr) => {
        let duration = $start.elapsed().as_secs_f64();
        let version = std::env::var("DEPLOYMENT_VERSION").unwrap_or("DEV".to_string());
        INCOMING_API
            .with_label_values(&[$method, $endpoint, $status, $code, version.as_str()])
            .observe(duration);
    };
}

#[macro_export]
macro_rules! termination {
    ($type_:expr, $start:expr) => {
        let duration = $start.elapsed().as_secs_f64();
        let version = std::env::var("DEPLOYMENT_VERSION").unwrap_or("DEV".to_string());
        TERMINATION
            .with_label_values(&[$type_, version.as_str()])
            .observe(duration);
    };
}

/// Macro that observes the latency of a queue drainer process.
///
/// This macro measures the time taken for a queue drainer to process its items and updates the `QUEUE_DRAINER_LATENCY` histogram.
///
/// # Arguments
///
/// * `$type` - Type or category of the queue drainer.
/// * `$start` - The time when the queue drainer started processing.
#[macro_export]
macro_rules! queue_drainer_latency {
    ($start:expr, $end:expr) => {
        let duration = abs_diff_utc_as_sec($start, $end);
        QUEUE_DRAINER_LATENCY
            .with_label_values(&[])
            .observe(duration);
    };
}

/// Initializes and returns a `PrometheusMetrics` instance configured for the application.
///
/// This function sets up Prometheus metrics for various application processes, including incoming and external API requests, queue counters, and queue drainer latencies.
/// It also provides an endpoint (`/metrics`) for Prometheus to scrape these metrics.
///
/// # Examples
///
/// ```norun
/// fn main() {
///     HttpServer::new(move || {
///         App::new()
///             .wrap(prometheus_metrics()) // Using the prometheus_metrics function
///     })
///     .bind("127.0.0.1:8080").unwrap()
///     .run();
/// }
/// ```
///
/// # Returns
///
/// * `PrometheusMetrics` - A configured instance that collects and exposes the metrics.
///
/// # Panics
///
/// * If there's a failure initializing metrics, registering metrics to the Prometheus registry, or any other unexpected error during the setup.
pub fn prometheus_metrics() -> PrometheusMetrics {
    let prometheus = init_prometheus_metrics();

    prometheus
        .registry
        .register(Box::new(INCOMING_API.to_owned()))
        .expect("Failed to register incoming API metrics");

    prometheus
        .registry
        .register(Box::new(QUEUE_DRAINER_LATENCY.to_owned()))
        .expect("Failed to register queue drainer latency metrics");

    prometheus
        .registry
        .register(Box::new(TOTAL_LOCATION_UPDATES.to_owned()))
        .expect("Failed to register total location updates metrics");

    prometheus
        .registry
        .register(Box::new(TERMINATION.to_owned()))
        .expect("Failed to register termination metrics");

    prometheus
}
