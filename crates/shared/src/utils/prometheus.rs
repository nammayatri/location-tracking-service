use actix_web_prom::{PrometheusMetrics, PrometheusMetricsBuilder};
use prometheus::{opts, register_histogram_vec, register_int_counter, HistogramVec, IntCounter};

pub static INCOMING_API: once_cell::sync::Lazy<HistogramVec> = once_cell::sync::Lazy::new(|| {
    register_histogram_vec!(
        opts!("incoming_api", "Incoming API requests").into(),
        &["method", "handler", "status_code", "code", "version"]
    )
    .expect("Failed to register incoming API metrics")
});

pub static CALL_EXTERNAL_API: once_cell::sync::Lazy<HistogramVec> =
    once_cell::sync::Lazy::new(|| {
        register_histogram_vec!(
            opts!("call_external_api", "Call external API requests").into(),
            &["method", "url", "status"]
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
macro_rules! call_external_api {
    ($method:expr, $url:expr, $status:expr, $start:expr) => {
        let duration = $start.elapsed().as_secs_f64();
        CALL_EXTERNAL_API
            .with_label_values(&[$method, $url, $status])
            .observe(duration);
    };
}

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
}
