use actix_web_prom::{PrometheusMetrics, PrometheusMetricsBuilder};
use prometheus::{opts, register_histogram_vec, HistogramVec};

pub static INCOMING_API: once_cell::sync::Lazy<HistogramVec> = once_cell::sync::Lazy::new(|| {
    register_histogram_vec!(
        opts!("incoming_api", "Incoming API requests").into(),
        &["method", "endpoint", "status"]
    )
    .expect("Failed to register incoming API metrics")
});

pub static CALL_EXTERNAL_API: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            opts!("call_external_api", "Call external API requests").into(),
            &["method", "host", "endpoint", "status"]
        )
        .expect("Failed to register call external API metrics")
    });

#[macro_export]
macro_rules! incoming_api {
    ($method:expr, $endpoint:expr, $status:expr, $start:expr) => {
        let duration = $start.elapsed().as_secs_f64();
        INCOMING_API
            .with_label_values(&[$method, $endpoint, $status])
            .observe(duration);
    };
}

#[macro_export]
macro_rules! call_external_api {
    ($method:expr, $host:expr, $endpoint:expr, $status:expr, $start:expr) => {
        let duration = $start.elapsed().as_secs_f64();
        CALL_EXTERNAL_API
            .with_label_values(&[$method, $host, $endpoint, $status])
            .observe(duration);
    };
}

pub fn prometheus_metrics() -> PrometheusMetrics {
    let prometheus = PrometheusMetricsBuilder::new("api")
        .endpoint("/metrics")
        .build()
        .unwrap();

    prometheus
        .registry
        .register(Box::new(INCOMING_API.clone()))
        .expect("Failed to register incoming API metrics");

    prometheus
        .registry
        .register(Box::new(CALL_EXTERNAL_API.clone()))
        .expect("Failed to register call external API metrics");

    prometheus
}
