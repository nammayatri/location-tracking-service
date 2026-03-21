/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

#![allow(clippy::expect_used)]

//! Connection pool health monitoring for Redis.
//!
//! Provides on-demand health checks and a background monitor that
//! periodically pings both writer and reader pools, recording latency
//! and connectivity status as Prometheus metrics.

use fred::prelude::ClientLike;
use once_cell::sync::Lazy;
use prometheus::{GaugeVec, Histogram, HistogramOpts, Opts};
use shared::redis::types::RedisConnectionPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tracing::{error, info, warn};

// -- Prometheus metrics --

static REDIS_POOL_HEALTHY: Lazy<GaugeVec> = Lazy::new(|| {
    GaugeVec::new(
        Opts::new(
            "redis_pool_healthy",
            "Whether the Redis pool is healthy (1) or not (0)",
        ),
        &["pool"],
    )
    .expect("redis_pool_healthy metric")
});

static REDIS_POOL_PING_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    Histogram::with_opts(
        HistogramOpts::new(
            "redis_pool_ping_latency_seconds",
            "Latency of Redis PING health check",
        )
        .buckets(vec![0.0005, 0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25]),
    )
    .expect("redis_pool_ping_latency metric")
});

static REDIS_POOL_CONNECTED_CLIENTS: Lazy<GaugeVec> = Lazy::new(|| {
    GaugeVec::new(
        Opts::new(
            "redis_pool_connected_clients",
            "Number of connected clients in the Redis pool",
        ),
        &["pool"],
    )
    .expect("redis_pool_connected_clients metric")
});

/// Registers pool health metrics with the given Prometheus registry.
pub fn register_health_metrics(registry: &prometheus::Registry) {
    let _ = registry.register(Box::new(REDIS_POOL_HEALTHY.clone()));
    let _ = registry.register(Box::new(REDIS_POOL_PING_LATENCY.clone()));
    let _ = registry.register(Box::new(REDIS_POOL_CONNECTED_CLIENTS.clone()));
}

/// Result of a single health check.
#[derive(Debug, Clone)]
pub struct PoolHealthStatus {
    pub is_healthy: bool,
    pub writer_connected: bool,
    pub reader_connected: bool,
    pub ping_latency_ms: f64,
    pub writer_active_connections: usize,
    pub reader_active_connections: usize,
}

/// Performs a point-in-time health check on both writer and reader pools.
pub async fn check_pool_health(redis: &RedisConnectionPool) -> PoolHealthStatus {
    let start = Instant::now();

    let (writer_result, reader_result) = tokio::join!(
        redis.writer_pool.ping::<String>(),
        redis.reader_pool.ping::<String>(),
    );

    let ping_latency = start.elapsed();
    let ping_latency_ms = ping_latency.as_secs_f64() * 1000.0;

    let writer_connected = writer_result.is_ok();
    let reader_connected = reader_result.is_ok();

    let writer_active = redis
        .writer_pool
        .active_connections()
        .await
        .map_or(0, |v| v.len());
    let reader_active = redis
        .reader_pool
        .active_connections()
        .await
        .map_or(0, |v| v.len());

    // Record metrics
    REDIS_POOL_PING_LATENCY.observe(ping_latency.as_secs_f64());
    REDIS_POOL_HEALTHY
        .with_label_values(&["writer"])
        .set(if writer_connected { 1.0 } else { 0.0 });
    REDIS_POOL_HEALTHY
        .with_label_values(&["reader"])
        .set(if reader_connected { 1.0 } else { 0.0 });
    REDIS_POOL_CONNECTED_CLIENTS
        .with_label_values(&["writer"])
        .set(writer_active as f64);
    REDIS_POOL_CONNECTED_CLIENTS
        .with_label_values(&["reader"])
        .set(reader_active as f64);

    PoolHealthStatus {
        is_healthy: writer_connected && reader_connected,
        writer_connected,
        reader_connected,
        ping_latency_ms,
        writer_active_connections: writer_active,
        reader_active_connections: reader_active,
    }
}

/// Spawns a background task that periodically checks pool health.
///
/// Logs warnings when connectivity is lost and info when it recovers.
/// Metrics are always updated regardless of health state.
pub fn spawn_health_monitor(redis: Arc<RedisConnectionPool>, interval: Duration) {
    tokio::spawn(async move {
        let mut was_healthy = true;

        loop {
            tokio::time::sleep(interval).await;

            let status = check_pool_health(&redis).await;

            if !status.is_healthy && was_healthy {
                error!(
                    writer = status.writer_connected,
                    reader = status.reader_connected,
                    latency_ms = status.ping_latency_ms,
                    "Redis pool health degraded"
                );
            } else if status.is_healthy && !was_healthy {
                info!(
                    latency_ms = status.ping_latency_ms,
                    writer_conns = status.writer_active_connections,
                    reader_conns = status.reader_active_connections,
                    "Redis pool health recovered"
                );
            } else if !status.is_healthy {
                warn!(
                    writer = status.writer_connected,
                    reader = status.reader_connected,
                    latency_ms = status.ping_latency_ms,
                    "Redis pool still unhealthy"
                );
            }

            was_healthy = status.is_healthy;
        }
    });
}
