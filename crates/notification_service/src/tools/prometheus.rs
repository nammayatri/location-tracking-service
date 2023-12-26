/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use actix_web_prom::{PrometheusMetrics, PrometheusMetricsBuilder};
use prometheus::{
    opts, register_histogram_vec, register_int_counter, register_int_gauge, HistogramVec,
    IntCounter, IntGauge,
};

pub static NOTIFICATION_LATENCY: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            opts!("notification_duration_seconds", "Notification Latency").into(),
            &["version"]
        )
        .expect("Failed to register notifiction latency metrics")
    });

pub static CONNECTED_CLIENTS: once_cell::sync::Lazy<IntGauge> = once_cell::sync::Lazy::new(|| {
    register_int_gauge!("connected_clients", "Connected Clients")
        .expect("Failed to register connected clients metrics")
});

pub static EXPIRED_NOTIFICATIONS: once_cell::sync::Lazy<IntCounter> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter!("expired_notifications", "Expired Notifications")
            .expect("Failed to register expired notifications metrics")
    });

pub static RETRIED_NOTIFICATIONS: once_cell::sync::Lazy<IntCounter> =
    once_cell::sync::Lazy::new(|| {
        register_int_counter!("retried_notifications", "Retried Notifications")
            .expect("Failed to register retried notifications metrics")
    });

#[macro_export]
macro_rules! notification_latency {
    ($start:expr) => {
        let now = Utc::now();
        let duration = diff_utc($start, now);
        let version = std::env::var("DEPLOYMENT_VERSION").unwrap_or("DEV".to_string());
        NOTIFICATION_LATENCY
            .with_label_values(&[version.as_str()])
            .observe(duration.num_seconds() as f64);
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
        .register(Box::new(NOTIFICATION_LATENCY.to_owned()))
        .expect("Failed to register notification latency metrics");

    prometheus
        .registry
        .register(Box::new(CONNECTED_CLIENTS.to_owned()))
        .expect("Failed to register connected clients metrics");

    prometheus
        .registry
        .register(Box::new(EXPIRED_NOTIFICATIONS.to_owned()))
        .expect("Failed to register expired notifications metrics");

    prometheus
        .registry
        .register(Box::new(RETRIED_NOTIFICATIONS.to_owned()))
        .expect("Failed to register retried notifications metrics");

    prometheus
}
