/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use actix_web_prom::PrometheusMetrics;
use prometheus::{
    opts, register_histogram, register_histogram_vec, register_int_counter,
    register_int_counter_vec, register_int_gauge, Histogram, HistogramVec, IntCounter,
    IntCounterVec, IntGauge,
};
pub use shared::tools::prometheus::*;

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

pub static DRAINER_FLUSH_DURATION: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(
            "drainer_flush_duration_seconds",
            "Time taken to flush drainer batch to Redis",
            vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
        )
        .expect("Failed to register drainer flush duration metrics")
    });

pub static DRAINER_BATCH_SIZE: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(
            "drainer_batch_size",
            "Number of items per drainer flush",
            vec![1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0]
        )
        .expect("Failed to register drainer batch size metrics")
    });

pub static DRAINER_FLUSHES_TOTAL: once_cell::sync::Lazy<IntCounterVec> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter_vec!(
            "drainer_flushes_total",
            "Total number of drainer flushes by trigger type",
            &["trigger"]
        )
        .expect("Failed to register drainer flushes total metrics")
    });

pub static DRAINER_LAG_SECONDS: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(
            "drainer_lag_seconds",
            "Time from first item in batch to flush completion",
            vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]
        )
        .expect("Failed to register drainer lag metrics")
    });

pub static TOKIO_WORKER_THREADS_GAUGE: once_cell::sync::Lazy<IntGauge> =
    once_cell::sync::Lazy::new(|| {
        register_int_gauge!(
            "tokio_runtime_worker_threads",
            "Number of Tokio runtime worker threads"
        )
        .expect("Failed to register tokio worker threads gauge")
    });

pub static ACTIX_HTTP_WORKERS_GAUGE: once_cell::sync::Lazy<IntGauge> =
    once_cell::sync::Lazy::new(|| {
        register_int_gauge!(
            "actix_http_workers",
            "Number of actix-web HTTP worker threads"
        )
        .expect("Failed to register actix HTTP workers gauge")
    });

pub static GPS_UPDATES_IGNORED_NO_ACTIVE_RIDE: once_cell::sync::Lazy<IntCounter> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter!(
            "gps_updates_ignored_no_active_ride",
            "GPS updates ignored because vehicle has no active ride"
        )
        .expect("Failed to register GPS ignored updates metrics")
    });

pub static REQUEST_DURATION_SECONDS: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            "lts_request_duration_seconds",
            "HTTP request duration per endpoint",
            &["endpoint", "method"],
            vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
        )
        .expect("Failed to register request duration metrics")
    });

pub static REDIS_OP_DURATION_SECONDS: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            "lts_redis_op_duration_seconds",
            "Redis operation latency by command",
            &["command"],
            vec![0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
        )
        .expect("Failed to register redis op duration metrics")
    });

pub static DB_WRITE_DURATION_SECONDS: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            "lts_db_write_duration_seconds",
            "DB/Kafka write operation latency",
            &["operation"],
            vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
        )
        .expect("Failed to register db write duration metrics")
    });

pub static ACTIVE_CONNECTIONS: once_cell::sync::Lazy<IntGauge> = once_cell::sync::Lazy::new(|| {
    register_int_gauge!(
        "lts_active_connections",
        "Number of currently active HTTP connections"
    )
    .expect("Failed to register active connections gauge")
});

pub static ERROR_TOTAL: once_cell::sync::Lazy<IntCounterVec> = once_cell::sync::Lazy::new(|| {
    register_int_counter_vec!(
        "lts_errors_total",
        "Total number of errors by endpoint and type",
        &["endpoint", "error_type"]
    )
    .expect("Failed to register error total metrics")
});

pub static REQUEST_THROUGHPUT_TOTAL: once_cell::sync::Lazy<IntCounterVec> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter_vec!(
            "lts_requests_total",
            "Total request count by endpoint and method",
            &["endpoint", "method"]
        )
        .expect("Failed to register request throughput metrics")
    });

pub static APP_ERRORS_TOTAL: once_cell::sync::Lazy<IntCounterVec> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter_vec!(
            "app_errors_total",
            "Total application errors by error code and HTTP status",
            &["error_code", "status"]
        )
        .expect("Failed to register app errors total metrics")
    });

pub static REDIS_ERRORS_TOTAL: once_cell::sync::Lazy<IntCounterVec> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter_vec!(
            "redis_errors_total",
            "Total Redis operation errors by operation",
            &["operation"]
        )
        .expect("Failed to register redis errors total metrics")
    });

pub static KAFKA_ERRORS_TOTAL: once_cell::sync::Lazy<IntCounterVec> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter_vec!(
            "kafka_errors_total",
            "Total Kafka push errors by producer type",
            &["producer"]
        )
        .expect("Failed to register kafka errors total metrics")
    });

pub static EXTERNAL_API_ERRORS_TOTAL: once_cell::sync::Lazy<IntCounterVec> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter_vec!(
            "external_api_errors_total",
            "Total external API call errors by endpoint",
            &["endpoint"]
        )
        .expect("Failed to register external API errors total metrics")
    });

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
        .register(Box::new(QUEUE_DRAINER_LATENCY.to_owned()))
        .expect("Failed to register queue drainer latency metrics");

    prometheus
        .registry
        .register(Box::new(TOTAL_LOCATION_UPDATES.to_owned()))
        .expect("Failed to register total location updates metrics");

    prometheus
        .registry
        .register(Box::new(GPS_UPDATES_IGNORED_NO_ACTIVE_RIDE.to_owned()))
        .expect("Failed to register GPS ignored updates metrics");

    prometheus
        .registry
        .register(Box::new(DRAINER_FLUSH_DURATION.to_owned()))
        .expect("Failed to register drainer flush duration metrics");

    prometheus
        .registry
        .register(Box::new(DRAINER_BATCH_SIZE.to_owned()))
        .expect("Failed to register drainer batch size metrics");

    prometheus
        .registry
        .register(Box::new(DRAINER_FLUSHES_TOTAL.to_owned()))
        .expect("Failed to register drainer flushes total metrics");

    prometheus
        .registry
        .register(Box::new(DRAINER_LAG_SECONDS.to_owned()))
        .expect("Failed to register drainer lag metrics");

    prometheus
        .registry
        .register(Box::new(TOKIO_WORKER_THREADS_GAUGE.to_owned()))
        .expect("Failed to register tokio worker threads gauge");

    prometheus
        .registry
        .register(Box::new(ACTIX_HTTP_WORKERS_GAUGE.to_owned()))
        .expect("Failed to register actix HTTP workers gauge");

    prometheus
        .registry
        .register(Box::new(REQUEST_DURATION_SECONDS.to_owned()))
        .expect("Failed to register request duration metrics");

    prometheus
        .registry
        .register(Box::new(REDIS_OP_DURATION_SECONDS.to_owned()))
        .expect("Failed to register redis op duration metrics");

    prometheus
        .registry
        .register(Box::new(DB_WRITE_DURATION_SECONDS.to_owned()))
        .expect("Failed to register db write duration metrics");

    prometheus
        .registry
        .register(Box::new(ACTIVE_CONNECTIONS.to_owned()))
        .expect("Failed to register active connections gauge");

    prometheus
        .registry
        .register(Box::new(ERROR_TOTAL.to_owned()))
        .expect("Failed to register error total metrics");

    prometheus
        .registry
        .register(Box::new(REQUEST_THROUGHPUT_TOTAL.to_owned()))
        .expect("Failed to register request throughput metrics");

    prometheus
        .registry
        .register(Box::new(APP_ERRORS_TOTAL.to_owned()))
        .expect("Failed to register app errors total metrics");

    prometheus
        .registry
        .register(Box::new(REDIS_ERRORS_TOTAL.to_owned()))
        .expect("Failed to register redis errors total metrics");

    prometheus
        .registry
        .register(Box::new(KAFKA_ERRORS_TOTAL.to_owned()))
        .expect("Failed to register kafka errors total metrics");

    prometheus
        .registry
        .register(Box::new(EXTERNAL_API_ERRORS_TOTAL.to_owned()))
        .expect("Failed to register external API errors total metrics");

    crate::redis::health::register_health_metrics(&prometheus.registry);

    prometheus
}
