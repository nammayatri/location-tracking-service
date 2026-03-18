/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::kafka::push_to_kafka;
use crate::environment::BatchConfig;
use crate::queue_drainer_latency;
use crate::special_location::{lookup_special_location, SpecialLocationCache};
use crate::tools::prometheus::{
    DRAINER_BATCH_SIZE_ACTUAL, DRAINER_QUEUE_DEPTH, DRAINER_QUEUE_SATURATION_RATIO,
    QUEUE_DRAINER_LATENCY, REDIS_PIPELINE_LATENCY_SECONDS, TOTAL_LOCATION_UPDATES,
};
use crate::{
    common::{
        types::*,
        utils::{abs_diff_utc_as_sec, get_bucket_from_timestamp},
    },
    redis::{
        commands::{
            add_driver_to_special_location_zset, batch_check_drivers_on_ride,
            batch_get_driver_queue_trackings, push_drainer_driver_location, DriverQueueTracking,
        },
        keys::{driver_loc_bucket_key, driver_queue_tracking_key, special_location_queue_key},
    },
};
use chrono::{DateTime, Utc};
use fred::prelude::{KeysInterface, SortedSetsInterface};
use fred::types::{GeoPosition, GeoValue, RedisValue, SetOptions};
use rdkafka::producer::FutureProducer;
use rustc_hash::FxHashMap;
use serde::Serialize;
use shared::redis::types::RedisConnectionPool;
use shared::termination;
use shared::tools::prometheus::TERMINATION;
use std::cmp::max;
use std::{
    cmp::min,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tokio::time::Instant;
use tracing::{error, info};

/// Telemetry message emitted for batches with trace ids.
#[derive(Serialize)]
struct BatchTelemetryMessage {
    batch_trace_id: String,
    stage: String,
    point_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_batched_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lts_received_at: Option<i64>,
    lts_drained_at: i64,
    drainer_queue_depth: usize,
}

/// Queue action collected during the drainer loop.
enum QueueAction {
    Enter {
        merchant_id: String,
        driver_id: String,
        special_location_id: String,
        vehicle_type: String,
        timestamp: f64,
    },
    PossibleExit {
        merchant_id: String,
        driver_id: String,
    },
}

/// Process queue actions (enter/exit) in the background using batched Redis operations.
/// Uses MGET to fetch all tracking keys in one round trip, then a pipeline for all writes.
/// Errors are logged but never block the caller.
async fn drain_queue_actions(
    actions: Vec<QueueAction>,
    redis: &RedisConnectionPool,
    queue_expiry: u64,
) {
    let expiry_i64 = queue_expiry as i64;

    // Step 1: Collect all (merchant_id, driver_id) pairs to fetch tracking info via MGET
    let pairs: Vec<(&str, &str)> = actions
        .iter()
        .map(|a| match a {
            QueueAction::Enter {
                merchant_id,
                driver_id,
                ..
            } => (merchant_id.as_str(), driver_id.as_str()),
            QueueAction::PossibleExit {
                merchant_id,
                driver_id,
            } => (merchant_id.as_str(), driver_id.as_str()),
        })
        .collect();

    let (trackings, on_ride_flags) = tokio::join!(
        batch_get_driver_queue_trackings(redis, &pairs),
        batch_check_drivers_on_ride(redis, &pairs)
    );

    let trackings = match trackings {
        Ok(t) => t,
        Err(e) => {
            error!(tag = "[Queue Batch MGET]", error = %e);
            return;
        }
    };

    let on_ride_flags = match on_ride_flags {
        Ok(f) => f,
        Err(e) => {
            error!(tag = "[Queue Batch On Ride Check]", error = %e);
            vec![false; actions.len()]
        }
    };

    // Step 2: Build all write commands into a single pipeline
    let pipeline = redis.writer_pool.next().pipeline();

    for ((action, old_tracking), is_on_ride) in actions
        .iter()
        .zip(trackings.iter())
        .zip(on_ride_flags.iter())
    {
        match action {
            QueueAction::Enter {
                merchant_id,
                driver_id,
                special_location_id,
                vehicle_type,
                timestamp,
            } => {
                if *is_on_ride {
                    continue;
                }
                let new_tracking = DriverQueueTracking {
                    special_location_id: special_location_id.clone(),
                    vehicle_type: vehicle_type.clone(),
                };

                // If driver was in a different queue, remove from old one
                if let Some(ref old) = old_tracking {
                    if *old != new_tracking {
                        let old_key =
                            special_location_queue_key(&old.special_location_id, &old.vehicle_type);
                        let old_member = serde_json::to_string(driver_id.as_str())
                            .unwrap_or_else(|_| driver_id.clone());
                        let _ = pipeline
                            .zrem::<RedisValue, _, _>(&old_key, old_member)
                            .await;
                    }
                }

                // ZADD NX to new queue
                let queue_key = special_location_queue_key(special_location_id, vehicle_type);
                let member =
                    serde_json::to_string(driver_id.as_str()).unwrap_or_else(|_| driver_id.clone());
                let _ = pipeline
                    .zadd::<RedisValue, _, _>(
                        &queue_key,
                        Some(SetOptions::NX),
                        None,
                        false,
                        false,
                        (*timestamp, member.as_str()),
                    )
                    .await;
                let _ = pipeline.expire::<(), _>(&queue_key, expiry_i64).await;

                // SET tracking key without expiry — only deleted on explicit queue exit.
                // This ensures we always know which queue a driver is in, even if
                // location updates stop temporarily.
                let tracking_key = driver_queue_tracking_key(merchant_id, driver_id);
                if let Ok(value) = serde_json::to_string(&new_tracking) {
                    let _ = pipeline
                        .set::<RedisValue, _, _>(&tracking_key, value, None, None, false)
                        .await;
                }
            }
            QueueAction::PossibleExit {
                merchant_id,
                driver_id,
            } => {
                if let Some(ref tracking) = old_tracking {
                    // ZREM from queue
                    let queue_key = special_location_queue_key(
                        &tracking.special_location_id,
                        &tracking.vehicle_type,
                    );
                    let member = serde_json::to_string(driver_id.as_str())
                        .unwrap_or_else(|_| driver_id.clone());
                    let _ = pipeline.zrem::<RedisValue, _, _>(&queue_key, member).await;

                    // DEL tracking key
                    let tracking_key = driver_queue_tracking_key(merchant_id, driver_id);
                    let _ = pipeline.del::<RedisValue, _>(&tracking_key).await;
                }
            }
        }
    }

    // Step 3: Execute entire pipeline in one round trip
    let result: Result<Vec<RedisValue>, _> = pipeline.all().await;
    if let Err(e) = result {
        error!(tag = "[Queue Pipeline Execute]", error = %e);
    }
}

/// Asynchronously drains driver locations to a Redis server.
///
/// This utility function is primarily intended to be used within `run_drainer`
/// to handle the logic of taking the queued driver locations and pushing them
/// to a Redis server. If there's any error during the push operation to Redis,
/// an error log is emitted.
///
/// # Arguments
///
/// * `driver_locations` - A reference to a hashmap containing driver locations.
/// * `bucket_expiry` - The expiration time for a bucket.
/// * `redis` - A reference to the Redis connection pool.
///
/// # Example
///
/// This function is typically used within the context of `run_drainer`:
///
/// ```ignore
/// async fn run_drainer(...) {
///     ...
///     drain_driver_locations(&driver_locations, bucket_expiry, &redis).await;
///     ...
/// }
/// ```
async fn drain_driver_locations(
    driver_locations: &DriversLocationMap,
    special_location_entries: &FxHashMap<String, Vec<(DriverId, u64, f64)>>,
    queue_actions: Vec<QueueAction>,
    bucket_expiry: i64,
    queue_expiry: u64,
    redis: &Arc<RedisConnectionPool>,
) {
    info!(
        tag = "[Queued Entries For Draining]",
        "Queue: {:?}\nPushing to redis server", driver_locations
    );

    let redis_start = Instant::now();
    if let Err(err) = push_drainer_driver_location(driver_locations, &bucket_expiry, redis).await {
        error!(tag = "[Error Pushing To Redis]", error = %err);
    }
    REDIS_PIPELINE_LATENCY_SECONDS.observe(redis_start.elapsed().as_secs_f64());

    for (key, entries) in special_location_entries {
        for (driver_id, bucket, score) in entries {
            let ts = chrono::TimeZone::timestamp_opt(&Utc, *score as i64, 0)
                .single()
                .unwrap_or_else(Utc::now);
            if let Err(err) = add_driver_to_special_location_zset(
                redis,
                key,
                *bucket,
                driver_id,
                &TimeStamp(ts),
                bucket_expiry,
            )
            .await
            {
                error!(tag = "[Error Adding To Special Location ZSET]", key = %key, error = %err);
            }
        }
    }

    // Fire-and-forget queue actions in a spawned task so they don't block the main drain.
    if !queue_actions.is_empty() {
        let redis = Arc::clone(redis);
        tokio::spawn(async move {
            drain_queue_actions(queue_actions, &redis, queue_expiry).await;
        });
    }
}

/// Cleans up the drainer after data has been processed or flushed.
///
/// This function is responsible for resetting counters, clearing driver locations,
/// and updating the start time for the next batch of data.
///
/// # Arguments
///
/// * `drainer_size` - A mutable reference to the current size of the drainer.
/// * `driver_locations` - A mutable reference to the map storing driver locations.
/// * `drainer_queue_min_max_timestamp_range` - A mutable reference to the minimum and maximum durations for draining the current data batch.
///
fn cleanup_drainer(
    drainer_size: &mut usize,
    driver_locations: &mut DriversLocationMap,
    special_location_zset_entries: &mut FxHashMap<String, Vec<(DriverId, u64, f64)>>,
    drainer_queue_min_max_timestamp_range: &mut Option<(DateTime<Utc>, DateTime<Utc>)>,
) {
    if let Some((min_drainer_ts, max_drainer_ts)) = drainer_queue_min_max_timestamp_range {
        queue_drainer_latency!(*min_drainer_ts, *max_drainer_ts);
    };
    *drainer_size = 0;
    *driver_locations = FxHashMap::default();
    *special_location_zset_entries = FxHashMap::default();
    *drainer_queue_min_max_timestamp_range = None;
}

/// Redis key for persisted drainer queue on shutdown.
const DRAINER_PERSIST_KEY: &str = "lts:drainer:persisted_queue";

/// Persist remaining items from receiver to Redis on shutdown.
async fn persist_drainer_queue(
    rx: &mut mpsc::Receiver<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    redis: &RedisConnectionPool,
) {
    let mut items: Vec<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)> = Vec::new();
    while let Ok(item) = rx.try_recv() {
        items.push(item);
    }
    if items.is_empty() {
        return;
    }

    // Serialize items using a simple JSON array of tuples
    #[derive(Serialize)]
    struct PersistedItem {
        merchant_id: String,
        city: String,
        vehicle_type: String,
        created_at: DateTime<Utc>,
        merchant_operating_city_id: String,
        latitude: f64,
        longitude: f64,
        timestamp: DateTime<Utc>,
        driver_id: String,
    }

    let persisted: Vec<PersistedItem> = items
        .into_iter()
        .map(
            |(dims, Latitude(lat), Longitude(lon), TimeStamp(ts), DriverId(did))| PersistedItem {
                merchant_id: dims.merchant_id.0,
                city: dims.city.0,
                vehicle_type: dims.vehicle_type.to_string(),
                created_at: dims.created_at,
                merchant_operating_city_id: dims.merchant_operating_city_id.0,
                latitude: lat,
                longitude: lon,
                timestamp: ts,
                driver_id: did,
            },
        )
        .collect();

    match serde_json::to_string(&persisted) {
        Ok(json) => {
            let result: Result<(), _> = redis
                .set_key_as_str(DRAINER_PERSIST_KEY, json.as_str(), 300_u32)
                .await;
            match result {
                Ok(()) => info!(
                    tag = "[Drainer Persist]",
                    "Persisted {} items to Redis",
                    persisted.len()
                ),
                Err(e) => error!(tag = "[Drainer Persist Error]", error = %e),
            }
        }
        Err(e) => error!(tag = "[Drainer Persist Serialize Error]", error = %e),
    }
}

/// Restore persisted queue items from Redis on startup.
async fn restore_drainer_queue(
    tx: &mpsc::Sender<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    redis: &RedisConnectionPool,
) {
    #[derive(serde::Deserialize)]
    struct PersistedItem {
        merchant_id: String,
        city: String,
        vehicle_type: String,
        created_at: DateTime<Utc>,
        merchant_operating_city_id: String,
        latitude: f64,
        longitude: f64,
        timestamp: DateTime<Utc>,
        driver_id: String,
    }

    let json: Result<Option<String>, _> = redis.get_key_as_str(DRAINER_PERSIST_KEY).await;
    let json = match json {
        Ok(Some(j)) => j,
        _ => return,
    };

    let items: Vec<PersistedItem> = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(e) => {
            error!(tag = "[Drainer Restore Parse Error]", error = %e);
            return;
        }
    };

    let count = items.len();
    for item in items {
        let vehicle_type = item
            .vehicle_type
            .parse::<VehicleType>()
            .unwrap_or(VehicleType::SEDAN);
        let dims = Dimensions {
            merchant_id: MerchantId(item.merchant_id),
            city: CityName(item.city),
            vehicle_type,
            created_at: item.created_at,
            merchant_operating_city_id: MerchantOperatingCityId(item.merchant_operating_city_id),
        };
        let _ = tx
            .send((
                dims,
                Latitude(item.latitude),
                Longitude(item.longitude),
                TimeStamp(item.timestamp),
                DriverId(item.driver_id),
            ))
            .await;
    }

    // Delete the persisted key after restore
    let _: Result<(), _> = redis.delete_key(DRAINER_PERSIST_KEY).await;
    info!(
        tag = "[Drainer Restore]",
        "Restored {} items from Redis",
        count
    );
}

/// Compute dynamic batch capacity based on queue saturation.
fn compute_dynamic_capacity(
    current_capacity: usize,
    default_capacity: usize,
    saturation: f64,
) -> usize {
    let max_capacity: usize = 50;
    if saturation > 0.7 {
        // Scale up by 50%, capped at max
        min((current_capacity * 3) / 2, max_capacity)
    } else if saturation < 0.3 {
        // Reset to default
        default_capacity
    } else {
        // Hysteresis: keep current
        current_capacity
    }
}

/// Compute adaptive drain interval based on queue saturation.
fn compute_dynamic_interval(
    current_delay_secs: u64,
    default_delay_secs: u64,
    saturation: f64,
    sustained_low_ticks: u64,
) -> u64 {
    if saturation > 0.5 {
        // Queue > 50% full: halve interval, min 2s
        max(current_delay_secs / 2, 2)
    } else if saturation < 0.2 && sustained_low_ticks >= 5 {
        // Queue < 20% for sustained period: increase, max 60s
        min((current_delay_secs * 3) / 2, 60)
    } else {
        // Default if nothing special
        default_delay_secs
    }
}

/// Emit batch telemetry messages for traced items in the current drain batch.
async fn emit_batch_telemetry(
    drainer_size: usize,
    queue_depth: usize,
    producer: &Option<FutureProducer>,
    secondary_producer: &Option<FutureProducer>,
    topic: &str,
    telemetry_sample_rate: f64,
) {
    // Only emit if we have a producer and a non-zero sample rate
    if producer.is_none() || telemetry_sample_rate <= 0.0 {
        return;
    }

    // Sample-based: emit a summary telemetry event for this drain cycle
    let should_emit = if telemetry_sample_rate >= 1.0 {
        true
    } else {
        rand::random::<f64>() < telemetry_sample_rate
    };

    if !should_emit {
        return;
    }

    let now = Utc::now().timestamp_millis();
    let message = BatchTelemetryMessage {
        batch_trace_id: format!("drain-cycle-{}", now),
        stage: "lts_drainer".to_string(),
        point_count: drainer_size,
        client_batched_at: None,
        lts_received_at: None,
        lts_drained_at: now,
        drainer_queue_depth: queue_depth,
    };

    if let Err(err) = push_to_kafka(producer, secondary_producer, topic, "telemetry", message).await
    {
        error!(tag = "[Batch Telemetry Kafka Error]", error = %err.message());
    }
}

/// Asynchronously runs a drainer.
///
/// This function listens to incoming driver location data and periodically drains it
/// to a Redis data store.
///
/// # Arguments
///
/// * `rx` - A receiver for incoming driver location data.
/// * `graceful_termination_requested` - An atomic flag indicating if termination is requested.
/// * `drainer_capacity` - Maximum capacity before forcefully draining data.
/// * `drainer_delay` - Time interval for periodic draining.
/// * `bucket_size` - The size of each time bucket.
/// * `near_by_bucket_threshold` - A threshold for nearby buckets.
/// * `redis` - Redis connection pool for non-persistent storage.
///
#[allow(clippy::too_many_arguments)]
pub async fn run_drainer(
    mut rx: mpsc::Receiver<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    graceful_termination_requested: Arc<AtomicBool>,
    drainer_capacity: usize,
    drainer_delay: u64,
    bucket_size: u64,
    near_by_bucket_threshold: u64,
    redis: Arc<RedisConnectionPool>,
    special_location_cache: Option<SpecialLocationCache>,
    enable_special_location_bucketing: bool,
    queue_expiry_seconds: u64,
    drainer_queue_depth: Arc<AtomicUsize>,
    drainer_queue_capacity: usize,
    batch_config: Arc<RwLock<BatchConfig>>,
    producer: Option<FutureProducer>,
    secondary_producer: Option<FutureProducer>,
    batch_telemetry_topic: String,
) {
    // Restoration of persisted queue is done externally in main.rs
    // before spawning the drainer via restore_persisted_queue().

    let mut driver_locations: DriversLocationMap = FxHashMap::default();
    let mut special_location_zset_entries: FxHashMap<String, Vec<(DriverId, u64, f64)>> =
        FxHashMap::default();
    let mut queue_actions: Vec<QueueAction> = Vec::new();
    let mut current_delay = drainer_delay;
    let mut timer = interval(Duration::from_secs(current_delay));
    let mut drainer_queue_min_max_timestamp_range = None;

    let mut drainer_size = 0;
    let mut dynamic_capacity = drainer_capacity;
    let mut sustained_low_ticks: u64 = 0;

    let bucket_expiry = (bucket_size * near_by_bucket_threshold) as i64;

    loop {
        if graceful_termination_requested.load(Ordering::Relaxed) {
            termination!("graceful-termination", Instant::now());
            info!(tag = "[Graceful Shutting Down]", length = %drainer_size);
            if drainer_size > 0 {
                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                let actions = std::mem::take(&mut queue_actions);
                drain_driver_locations(
                    &driver_locations,
                    &special_location_zset_entries,
                    actions,
                    bucket_expiry,
                    queue_expiry_seconds,
                    &redis,
                )
                .await;
                cleanup_drainer(
                    &mut drainer_size,
                    &mut driver_locations,
                    &mut special_location_zset_entries,
                    &mut drainer_queue_min_max_timestamp_range,
                );
            }
            // Persist remaining items in channel to Redis
            persist_drainer_queue(&mut rx, &redis).await;
            break;
        }

        // Compute queue saturation for dynamic adjustments
        let depth = drainer_queue_depth.load(Ordering::Relaxed);
        let saturation = if drainer_queue_capacity > 0 {
            depth as f64 / drainer_queue_capacity as f64
        } else {
            0.0
        };

        // Update Prometheus metrics
        DRAINER_QUEUE_DEPTH.set(depth as i64);
        DRAINER_QUEUE_SATURATION_RATIO.set(saturation);

        // Dynamic batch sizing
        {
            let config = batch_config.read().await;
            dynamic_capacity =
                compute_dynamic_capacity(dynamic_capacity, config.drainer_size, saturation);
        }

        // Adaptive drain interval
        let new_delay = {
            let config = batch_config.read().await;
            compute_dynamic_interval(current_delay, config.drainer_delay, saturation, sustained_low_ticks)
        };
        if new_delay != current_delay {
            current_delay = new_delay;
            timer = interval(Duration::from_secs(current_delay));
        }

        // Track sustained low saturation
        if saturation < 0.2 {
            sustained_low_ticks += 1;
        } else {
            sustained_low_ticks = 0;
        }

        // TODO :: When drainer is ticked that time all locations coming to reciever could get dropped and vice versa.
        tokio::select! {
            item = rx.recv() => {
                info!(tag = "[Recieved Entries For Queuing]");
                match item {
                    Some((Dimensions { merchant_id, city, vehicle_type, created_at, merchant_operating_city_id }, Latitude(latitude), Longitude(longitude), TimeStamp(timestamp), DriverId(driver_id))) => {
                        drainer_queue_depth.fetch_add(1, Ordering::Relaxed);

                        let bucket = get_bucket_from_timestamp(&bucket_size, TimeStamp(timestamp));

                        let skip_normal_drain = if let Some(ref cache) = special_location_cache {
                            let guard = cache.read().await;
                            if let Some(entry) = lookup_special_location(
                                &guard,
                                &merchant_operating_city_id,
                                &Latitude(latitude),
                                &Longitude(longitude),
                            ) {
                                if enable_special_location_bucketing {
                                    special_location_zset_entries
                                        .entry(entry.id.0.clone())
                                        .or_default()
                                        .push((
                                            DriverId(driver_id.clone()),
                                            bucket,
                                            timestamp.timestamp() as f64,
                                        ));
                                }
                                // Queue entry: if this special location is queue-enabled, enqueue driver
                                if entry.is_queue_enabled {
                                    queue_actions.push(QueueAction::Enter {
                                        merchant_id: merchant_id.0.clone(),
                                        driver_id: driver_id.clone(),
                                        special_location_id: entry.id.0.clone(),
                                        vehicle_type: vehicle_type.to_string(),
                                        timestamp: timestamp.timestamp() as f64,
                                    });
                                }
                                !entry.is_open_market_enabled
                            } else {
                                // No special location match → possible exit from queue
                                queue_actions.push(QueueAction::PossibleExit {
                                    merchant_id: merchant_id.0.clone(),
                                    driver_id: driver_id.clone(),
                                });
                                false
                            }
                        } else {
                            false
                        };

                        if !skip_normal_drain {
                            driver_locations
                                .entry(driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket))
                                .or_default()
                                .push(GeoValue {
                                    coordinates: GeoPosition {
                                        latitude,
                                        longitude,
                                    },
                                    member: driver_id.into(),
                                });
                        }
                        drainer_queue_min_max_timestamp_range = drainer_queue_min_max_timestamp_range.map_or(Some((created_at, created_at)), |(min_duration, max_duration)| Some((min(created_at, min_duration), max(created_at, max_duration))));
                        drainer_size += 1;

                        if drainer_size >= dynamic_capacity {
                            info!(tag = "[Force Draining Queue]", length = %drainer_size);
                            DRAINER_BATCH_SIZE_ACTUAL.observe(drainer_size as f64);

                            let current_depth = drainer_queue_depth.load(Ordering::Relaxed);
                            let telemetry_rate = {
                                let config = batch_config.read().await;
                                config.telemetry_sample_rate
                            };

                            emit_batch_telemetry(
                                drainer_size,
                                current_depth,
                                &producer,
                                &secondary_producer,
                                &batch_telemetry_topic,
                                telemetry_rate,
                            )
                            .await;

                            let actions = std::mem::take(&mut queue_actions);
                            drain_driver_locations(
                                &driver_locations,
                                &special_location_zset_entries,
                                actions,
                                bucket_expiry,
                                queue_expiry_seconds,
                                &redis,
                            )
                            .await;

                            drainer_queue_depth.fetch_sub(drainer_size, Ordering::Relaxed);

                            cleanup_drainer(
                                &mut drainer_size,
                                &mut driver_locations,
                                &mut special_location_zset_entries,
                                &mut drainer_queue_min_max_timestamp_range
                            );
                        }

                        TOTAL_LOCATION_UPDATES.inc()
                    },
                    None => {
                        error!("MPSC Sender is Disconnected, Should not happen while pod is serving traffic!");
                        break;
                    },
                }
            },
            _ = timer.tick() => {
                if drainer_size > 0 {
                    info!(tag = "[Draining Queue]", length = %drainer_size);
                    DRAINER_BATCH_SIZE_ACTUAL.observe(drainer_size as f64);

                    let current_depth = drainer_queue_depth.load(Ordering::Relaxed);
                    let telemetry_rate = {
                        let config = batch_config.read().await;
                        config.telemetry_sample_rate
                    };

                    emit_batch_telemetry(
                        drainer_size,
                        current_depth,
                        &producer,
                        &secondary_producer,
                        &batch_telemetry_topic,
                        telemetry_rate,
                    )
                    .await;

                    let actions = std::mem::take(&mut queue_actions);
                    drain_driver_locations(
                        &driver_locations,
                        &special_location_zset_entries,
                        actions,
                        bucket_expiry,
                        queue_expiry_seconds,
                        &redis,
                    )
                    .await;

                    drainer_queue_depth.fetch_sub(drainer_size, Ordering::Relaxed);

                    cleanup_drainer(
                        &mut drainer_size,
                        &mut driver_locations,
                        &mut special_location_zset_entries,
                        &mut drainer_queue_min_max_timestamp_range
                    );
                }
            },
        }
    }
}

/// Start a background task that polls Redis for hot-reloadable batch config.
pub fn spawn_batch_config_reloader(
    redis: Arc<RedisConnectionPool>,
    batch_config: Arc<RwLock<BatchConfig>>,
) {
    tokio::spawn(async move {
        let mut reload_interval = interval(Duration::from_secs(60));
        loop {
            reload_interval.tick().await;

            let key = "batch_pipeline_config:global";
            let json: Result<Option<String>, _> = redis.get_key(key).await;
            if let Ok(Some(json_str)) = json {
                #[derive(serde::Deserialize)]
                struct ReloadableConfig {
                    drainer_delay: Option<u64>,
                    drainer_size: Option<usize>,
                    batch_size: Option<i64>,
                    telemetry_sample_rate: Option<f64>,
                }

                if let Ok(reloaded) = serde_json::from_str::<ReloadableConfig>(&json_str) {
                    let mut config = batch_config.write().await;
                    if let Some(v) = reloaded.drainer_delay {
                        config.drainer_delay = v;
                    }
                    if let Some(v) = reloaded.drainer_size {
                        config.drainer_size = v;
                    }
                    if let Some(v) = reloaded.batch_size {
                        config.batch_size = v;
                    }
                    if let Some(v) = reloaded.telemetry_sample_rate {
                        config.telemetry_sample_rate = v;
                    }
                    info!(tag = "[Batch Config Reload]", "Updated batch config from Redis");
                }
            }
        }
    });
}

/// Public helper to restore drainer queue; called from main before spawning drainer.
pub async fn restore_persisted_queue(
    tx: &mpsc::Sender<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    redis: &RedisConnectionPool,
) {
    restore_drainer_queue(tx, redis).await;
}
