/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::queue_drainer_latency;
use crate::special_location::{lookup_special_location, SpecialLocationCache};
use crate::tools::prometheus::{
    DRAINER_BATCH_SIZE, DRAINER_FLUSHES_TOTAL, DRAINER_FLUSH_DURATION, DRAINER_LAG_SECONDS,
    QUEUE_DRAINER_LATENCY, TOTAL_LOCATION_UPDATES,
};
use crate::{
    common::{
        types::*,
        utils::{abs_diff_utc_as_sec, get_bucket_from_timestamp},
    },
    redis::{
        commands::{
            batch_add_drivers_to_special_location_zset, batch_check_drivers_on_ride,
            batch_get_driver_queue_trackings, push_drainer_driver_location, DriverQueueTracking,
        },
        keys::{driver_loc_bucket_key, driver_queue_tracking_key, special_location_queue_key},
    },
};
use chrono::{DateTime, Utc};
use fred::prelude::{KeysInterface, SortedSetsInterface};
use fred::types::{GeoPosition, GeoValue, RedisValue, SetOptions};
use rustc_hash::FxHashMap;
use shared::redis::types::RedisConnectionPool;
use shared::termination;
use shared::tools::prometheus::TERMINATION;
use std::cmp::max;
use std::{
    cmp::min,
    sync::atomic::{AtomicBool, Ordering},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::{error, info};

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

    // Run geo push and special location writes concurrently
    let (geo_result, _) = tokio::join!(
        push_drainer_driver_location(driver_locations, &bucket_expiry, redis),
        async {
            futures::future::join_all(
                special_location_entries.iter().map(|(key, entries)| async move {
                    if let Err(err) =
                        batch_add_drivers_to_special_location_zset(redis, key, entries, bucket_expiry)
                            .await
                    {
                        error!(tag = "[Error Adding To Special Location ZSET]", key = %key, error = %err);
                    }
                }),
            )
            .await;
        },
    );
    if let Err(err) = geo_result {
        error!(tag = "[Error Pushing To Redis]", error = %err);
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
    driver_locations.clear();
    special_location_zset_entries.clear();
    *drainer_queue_min_max_timestamp_range = None;
}

/// Records metrics after a drainer flush: flush duration, batch size, lag, and trigger type.
fn record_flush_metrics(
    batch_size: usize,
    flush_start: Instant,
    batch_start_time: Option<Instant>,
    trigger: &str,
) {
    DRAINER_FLUSH_DURATION.observe(flush_start.elapsed().as_secs_f64());
    DRAINER_BATCH_SIZE.observe(batch_size as f64);
    DRAINER_FLUSHES_TOTAL.with_label_values(&[trigger]).inc();
    if let Some(start) = batch_start_time {
        DRAINER_LAG_SECONDS.observe(start.elapsed().as_secs_f64());
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
) {
    // Pre-allocate maps with capacity hints based on expected batch size
    let mut driver_locations: DriversLocationMap =
        FxHashMap::with_capacity_and_hasher(drainer_capacity / 4, Default::default());
    let mut special_location_zset_entries: FxHashMap<String, Vec<(DriverId, u64, f64)>> =
        FxHashMap::default();
    let mut queue_actions: Vec<QueueAction> = Vec::new();
    let mut drainer_queue_min_max_timestamp_range = None;

    let mut drainer_size = 0;
    let mut batch_start_time: Option<Instant> = None;
    let flush_delay = Duration::from_secs(drainer_delay);

    let bucket_expiry = (bucket_size * near_by_bucket_threshold) as i64;

    loop {
        if graceful_termination_requested.load(Ordering::Relaxed) {
            termination!("graceful-termination", Instant::now());
            info!(tag = "[Graceful Shutting Down]", length = %drainer_size);
            if drainer_size > 0 {
                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                let actions = std::mem::take(&mut queue_actions);
                let flush_start = Instant::now();
                drain_driver_locations(
                    &driver_locations,
                    &special_location_zset_entries,
                    actions,
                    bucket_expiry,
                    queue_expiry_seconds,
                    &redis,
                )
                .await;
                record_flush_metrics(drainer_size, flush_start, batch_start_time, "shutdown");
                cleanup_drainer(
                    &mut drainer_size,
                    &mut driver_locations,
                    &mut special_location_zset_entries,
                    &mut drainer_queue_min_max_timestamp_range,
                );
            }
            break;
        }
        // Deadline-based flush: compute remaining time until first item in batch
        // has waited `flush_delay`. If no batch is pending, the timer branch is disabled.
        let time_until_flush = match batch_start_time {
            Some(start) => {
                let target = start + flush_delay;
                target.saturating_duration_since(Instant::now())
            }
            None => Duration::from_secs(86400), // irrelevant — guard disables this branch
        };

        tokio::select! {
            item = rx.recv() => {
                info!(tag = "[Recieved Entries For Queuing]");
                match item {
                    Some(first_item) => {
                        if batch_start_time.is_none() {
                            batch_start_time = Some(Instant::now());
                        }

                        // Batch-collect: drain remaining buffered items in one pass
                        let mut batch = Vec::with_capacity(drainer_capacity - drainer_size);
                        batch.push(first_item);
                        while batch.len() + drainer_size < drainer_capacity {
                            match rx.try_recv() {
                                Ok(item) => batch.push(item),
                                Err(_) => break,
                            }
                        }

                        let batch_len = batch.len();

                        // Acquire special location cache lock once for entire batch
                        let special_location_guard = match special_location_cache.as_ref() {
                            Some(cache) => Some(cache.read().await),
                            None => None,
                        };

                        for (Dimensions { merchant_id, city, vehicle_type, created_at, merchant_operating_city_id }, Latitude(latitude), Longitude(longitude), TimeStamp(timestamp), driver_id) in batch {
                            let bucket = get_bucket_from_timestamp(&bucket_size, TimeStamp(timestamp));

                            let skip_normal_drain = if let Some(ref guard) = special_location_guard {
                                if let Some(entry) = lookup_special_location(
                                    guard,
                                    &merchant_operating_city_id,
                                    &Latitude(latitude),
                                    &Longitude(longitude),
                                ) {
                                    if enable_special_location_bucketing {
                                        special_location_zset_entries
                                            .entry(entry.id.0.clone())
                                            .or_default()
                                            .push((
                                                driver_id.clone(),
                                                bucket,
                                                timestamp.timestamp() as f64,
                                            ));
                                    }
                                    // Queue entry: if this special location is queue-enabled, enqueue driver
                                    if entry.is_queue_enabled {
                                        queue_actions.push(QueueAction::Enter {
                                            merchant_id: merchant_id.0.clone(),
                                            driver_id: driver_id.0.clone(),
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
                                        driver_id: driver_id.0.clone(),
                                    });
                                    false
                                }
                            } else {
                                false
                            };

                            if !skip_normal_drain {
                                let key = driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket);
                                driver_locations
                                    .entry(key)
                                    .or_default()
                                    .push(GeoValue {
                                        coordinates: GeoPosition {
                                            latitude,
                                            longitude,
                                        },
                                        member: driver_id.0.into(),
                                    });
                            }
                            drainer_queue_min_max_timestamp_range = drainer_queue_min_max_timestamp_range.map_or(Some((created_at, created_at)), |(min_duration, max_duration)| Some((min(created_at, min_duration), max(created_at, max_duration))));
                        }
                        // Drop the lock before potential drain
                        drop(special_location_guard);

                        drainer_size += batch_len;
                        TOTAL_LOCATION_UPDATES.inc_by(batch_len as u64);

                        if drainer_size >= drainer_capacity {
                            info!(tag = "[Force Draining Queue]", length = %drainer_size);
                            let actions = std::mem::take(&mut queue_actions);
                            let flush_start = Instant::now();
                            drain_driver_locations(
                                &driver_locations,
                                &special_location_zset_entries,
                                actions,
                                bucket_expiry,
                                queue_expiry_seconds,
                                &redis,
                            )
                            .await;
                            record_flush_metrics(drainer_size, flush_start, batch_start_time, "capacity");
                            cleanup_drainer(
                                &mut drainer_size,
                                &mut driver_locations,
                                &mut special_location_zset_entries,
                                &mut drainer_queue_min_max_timestamp_range
                            );
                            batch_start_time = None;
                        }
                    },
                    None => {
                        error!("MPSC Sender is Disconnected, Should not happen while pod is serving traffic!");
                        break;
                    },
                }
            },
            _ = tokio::time::sleep(time_until_flush), if drainer_size > 0 => {
                info!(tag = "[Draining Queue]", length = %drainer_size);
                let actions = std::mem::take(&mut queue_actions);
                let flush_start = Instant::now();
                drain_driver_locations(
                    &driver_locations,
                    &special_location_zset_entries,
                    actions,
                    bucket_expiry,
                    queue_expiry_seconds,
                    &redis,
                )
                .await;
                record_flush_metrics(drainer_size, flush_start, batch_start_time, "deadline");
                cleanup_drainer(
                    &mut drainer_size,
                    &mut driver_locations,
                    &mut special_location_zset_entries,
                    &mut drainer_queue_min_max_timestamp_range
                );
                batch_start_time = None;
            },
        }
    }
}
