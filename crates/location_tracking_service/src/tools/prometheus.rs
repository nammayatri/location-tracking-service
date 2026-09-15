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
    histogram_opts, opts, register_histogram, register_histogram_vec, register_int_counter,
    register_int_counter_vec, Histogram, HistogramVec, IntCounter, IntCounterVec,
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

/// Counter of drivers removed from a special-location FIFO queue, labeled by
/// the eviction reason and the source special location.
///
/// Labels:
/// * `reason` — `hysteresis` (consecutive_exit_pings reached threshold),
///   `switch` (driver entered a different queue),
///   `manual` (admin-triggered removal via internal API),
///   or `offline` (driver flipped to OFFLINE mode)
/// * `special_location_id` — the queue the driver was evicted from
/// * `manual_reason` — sub-reason from the manual-remove request body
///   (e.g. `wrong_queue`, `complaint`). Empty string
///   for `hysteresis`/`switch` evictions and for
///   `manual` evictions where no reason was supplied.
///   **Keep the operator vocabulary bounded** — every
///   unique string here is a new prometheus time
///   series.
pub static QUEUE_EVICTIONS: once_cell::sync::Lazy<IntCounterVec> = once_cell::sync::Lazy::new(
    || {
        register_int_counter_vec!(
            opts!(
                "queue_evictions_total",
                "Total drivers evicted from special-location FIFO queues, by reason and source location"
            ),
            &["reason", "special_location_id", "manual_reason"]
        )
        .expect("Failed to register queue evictions metrics")
    },
);

/// Histogram of the number of drivers returned per `GET /internal/drivers/nearby`
/// request.
///
/// Observed once per request with the total driver count across all requested
/// vehicle types. The histogram exposes:
/// * `nearby_drivers_returned_count` — number of nearby requests served
/// * `nearby_drivers_returned_sum`   — total drivers returned across requests
/// * `nearby_drivers_returned_bucket` — distribution over the count buckets
///
/// Average drivers per request can be computed in prometheus as
/// `rate(nearby_drivers_returned_sum[5m]) / rate(nearby_drivers_returned_count[5m])`.
/// The buckets are tuned for counts (not latency), so the default histogram
/// buckets are overridden.
pub static NEARBY_DRIVERS_RETURNED: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(histogram_opts!(
            "nearby_drivers_returned",
            "Number of drivers returned per nearby drivers request",
            vec![0.0, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 750.0, 1000.0, 1500.0]
        ))
        .expect("Failed to register nearby drivers returned metrics")
    });

/// Histogram of how long a single location update sat in the in-memory drainer
/// before it was written to Redis, in seconds.
///
/// Observed once per buffered item at flush time as
/// `flush_start - Dimensions.created_at`, where `created_at` is stamped in the
/// ping handler immediately before the update is pushed onto the drainer.
///
/// This is distinct from `queue_drainer_latency`, which measures the timestamp
/// *spread* within a batch (newest minus oldest) rather than any actual wait.
///
/// Labels:
/// * `flush_reason` — `timer` (the `drainer_delay` tick fired),
///   `capacity` (buffer hit `drainer_size`), or
///   `shutdown` (force drain on SIGTERM/SIGINT)
///
pub static INMEM_QUEUE_DELAY: once_cell::sync::Lazy<HistogramVec> = once_cell::sync::Lazy::new(
    || {
        register_histogram_vec!(
            histogram_opts!(
                "inmem_queue_delay_seconds",
                "Seconds a location update spent in the in-memory drainer before being flushed to Redis",
                vec![
                    0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0, 120.0
                ]
            ),
            &["flush_reason"]
        )
        .expect("Failed to register inmem queue delay metrics")
    },
);

/// Histogram of the mpsc channel wait, in seconds — the time between the ping
/// handler calling `sender.send(..)` and the drainer's `rx.recv()` returning
/// that item.
///
/// Observed once per received item as `recv_time - Dimensions.created_at`. This
/// is the backpressure half of `inmem_queue_delay_seconds`: it stays near zero
/// while the drainer keeps up, and grows only when the channel
///  is full and `send` is made to await.
pub static INMEM_QUEUE_CHANNEL_WAIT: once_cell::sync::Lazy<Histogram> =
    once_cell::sync::Lazy::new(|| {
        register_histogram!(histogram_opts!(
            "inmem_queue_channel_wait_seconds",
            "Seconds a location update waited in the drainer mpsc channel before being buffered",
            vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]
        ))
        .expect("Failed to register inmem queue channel wait metrics")
    });

/// Histogram of how long one flush took to write to Redis, in seconds —
/// wall time around `drain_driver_locations`, covering the geo-bucket pipeline
/// and the special-location ZADDs, but not the fire-and-forget queue actions.
/// Labels: `flush_reason`, as for `inmem_queue_delay_seconds`.
pub static INMEM_QUEUE_DRAIN_DURATION: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            histogram_opts!(
                "inmem_queue_drain_duration_seconds",
                "Seconds taken to flush one in-memory drainer batch to Redis",
                vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]
            ),
            &["flush_reason"]
        )
        .expect("Failed to register inmem queue drain duration metrics")
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
        .register(Box::new(QUEUE_EVICTIONS.to_owned()))
        .expect("Failed to register queue evictions metrics");

    prometheus
        .registry
        .register(Box::new(NEARBY_DRIVERS_RETURNED.to_owned()))
        .expect("Failed to register nearby drivers returned metrics");

    prometheus
        .registry
        .register(Box::new(INMEM_QUEUE_DELAY.to_owned()))
        .expect("Failed to register inmem queue delay metrics");

    prometheus
        .registry
        .register(Box::new(INMEM_QUEUE_CHANNEL_WAIT.to_owned()))
        .expect("Failed to register inmem queue channel wait metrics");

    prometheus
        .registry
        .register(Box::new(INMEM_QUEUE_DRAIN_DURATION.to_owned()))
        .expect("Failed to register inmem queue drain duration metrics");

    prometheus
}
