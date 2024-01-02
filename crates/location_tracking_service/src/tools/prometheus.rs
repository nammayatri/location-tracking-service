/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use actix_web_prom::{PrometheusMetrics, PrometheusMetricsBuilder};
use prometheus::{opts, register_histogram_vec, register_int_counter, HistogramVec, IntCounter};

pub static INCOMING_API: once_cell::sync::Lazy<HistogramVec> = once_cell::sync::Lazy::new(|| {
    register_histogram_vec!(
        opts!("http_request_duration_seconds", "Incoming API requests").into(),
        &["method", "handler", "status_code", "code", "version"]
    )
    .expect("Failed to register incoming API metrics")
});

pub static CALL_EXTERNAL_API: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            opts!("external_request_duration", "Call external API requests").into(),
            &["method", "host", "service", "status"]
        )
        .expect("Failed to register call external API metrics")
    });

pub static QUEUE_COUNTER: once_cell::sync::Lazy<IntCounter> = once_cell::sync::Lazy::new(|| {
    register_int_counter!("queue_counter", "Queue Counter Montitoring")
        .expect("Failed to register queue counter metrics")
});

pub static NEW_RIDE_QUEUE_COUNTER: once_cell::sync::Lazy<IntCounter> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter!(
            "new_ride_queue_counter",
            "New Ride Queue Counter Montitoring"
        )
        .expect("Failed to register new ride queue counter metrics")
    });

pub static QUEUE_DRAINER_LATENCY: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            opts!("queue_drainer_latency", "Queue Drainer Montitoring").into(),
            &["type"]
        )
        .expect("Failed to register queue drainer latency metrics")
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

/// Macro that observes the duration of external API calls and logs metrics related to the external request.
///
/// This macro captures details of the external request like method, host, path, status, and the time taken for the external call.
/// It then updates the `CALL_EXTERNAL_API` histogram with these metrics.
///
/// # Arguments
///
/// * `$method` - The HTTP method of the external request.
/// * `$host` - The host or domain of the external service.
/// * `$path` - The path or endpoint of the external service.
/// * `$status` - The HTTP status code of the response from the external service.
/// * `$start` - The time when the external request was initiated.
#[macro_export]
macro_rules! call_external_api {
    ($method:expr, $host:expr, $path:expr, $status:expr, $start:expr) => {
        let duration = $start.elapsed().as_secs_f64();
        CALL_EXTERNAL_API
            .with_label_values(&[$method, $host, $path, $status])
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
    ($type:expr, $start:expr) => {
        let duration = Utc::now().signed_duration_since($start);
        let duration =
            duration.num_seconds() as f64 + (duration.num_milliseconds() % 1000) as f64 / 1000.0;
        QUEUE_DRAINER_LATENCY
            .with_label_values(&[$type])
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
    let prometheus = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .expect("Failed to create Prometheus Metrics");

    prometheus
        .registry
        .register(Box::new(INCOMING_API.to_owned()))
        .expect("Failed to register incoming API metrics");

    prometheus
        .registry
        .register(Box::new(CALL_EXTERNAL_API.to_owned()))
        .expect("Failed to register call external API metrics");

    prometheus
        .registry
        .register(Box::new(QUEUE_COUNTER.to_owned()))
        .expect("Failed to register queue counter metrics");

    prometheus
        .registry
        .register(Box::new(NEW_RIDE_QUEUE_COUNTER.to_owned()))
        .expect("Failed to register queue counter metrics");

    prometheus
        .registry
        .register(Box::new(QUEUE_DRAINER_LATENCY.to_owned()))
        .expect("Failed to register queue drainer latency metrics");

    prometheus
}
