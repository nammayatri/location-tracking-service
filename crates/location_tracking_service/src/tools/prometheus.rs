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
    opts, register_gauge, register_histogram, register_histogram_vec, register_int_counter,
    register_int_gauge, Gauge, Histogram, HistogramVec, IntCounter, IntGauge,
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

pub static GPS_UPDATES_IGNORED_NO_ACTIVE_RIDE: once_cell::sync::Lazy<IntCounter> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter!(
            "gps_updates_ignored_no_active_ride",
            "GPS updates ignored because vehicle has no active ride"
        )
        .expect("Failed to register GPS ignored updates metrics")
    });

pub static DRAINER_QUEUE_DEPTH: once_cell::sync::Lazy<IntGauge> =
    once_cell::sync::Lazy::new(|| {
        register_int_gauge!("drainer_queue_depth", "Current drainer queue depth")
            .expect("Failed to register drainer queue depth metric")
    });

pub static DRAINER_BATCH_SIZE_ACTUAL: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(
            "drainer_batch_size_actual",
            "Actual batch sizes used during draining",
            vec![1.0, 5.0, 10.0, 20.0, 30.0, 50.0, 100.0, 200.0]
        )
        .expect("Failed to register drainer batch size actual metric")
    });

pub static DRAINER_QUEUE_SATURATION_RATIO: once_cell::sync::Lazy<Gauge> =
    once_cell::sync::Lazy::new(|| {
        register_gauge!(
            "drainer_queue_saturation_ratio",
            "Ratio of drainer queue depth to capacity"
        )
        .expect("Failed to register drainer queue saturation ratio metric")
    });

pub static BATCH_LTS_RECEIVE_LATENCY_SECONDS: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(
            "batch_lts_receive_latency_seconds",
            "Latency from client batch to LTS receive in seconds",
            vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
        )
        .expect("Failed to register batch LTS receive latency metric")
    });

pub static REDIS_PIPELINE_LATENCY_SECONDS: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(
            "redis_pipeline_latency_seconds",
            "Latency of Redis pipeline operations in seconds",
            vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
        )
        .expect("Failed to register Redis pipeline latency metric")
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
        .register(Box::new(DRAINER_QUEUE_DEPTH.to_owned()))
        .expect("Failed to register drainer queue depth metric");

    prometheus
        .registry
        .register(Box::new(DRAINER_BATCH_SIZE_ACTUAL.to_owned()))
        .expect("Failed to register drainer batch size actual metric");

    prometheus
        .registry
        .register(Box::new(DRAINER_QUEUE_SATURATION_RATIO.to_owned()))
        .expect("Failed to register drainer queue saturation ratio metric");

    prometheus
        .registry
        .register(Box::new(BATCH_LTS_RECEIVE_LATENCY_SECONDS.to_owned()))
        .expect("Failed to register batch LTS receive latency metric");

    prometheus
        .registry
        .register(Box::new(REDIS_PIPELINE_LATENCY_SECONDS.to_owned()))
        .expect("Failed to register Redis pipeline latency metric");

    prometheus
}
