/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::queue_drainer_latency;
use crate::special_location::{lookup_special_location, SpecialLocationCache};
use crate::tools::prometheus::{QUEUE_DRAINER_LATENCY, QUEUE_EVICTIONS, TOTAL_LOCATION_UPDATES};
use crate::{
    common::{
        types::*,
        utils::{abs_diff_utc_as_sec, get_bucket_from_timestamp},
    },
    redis::{
        commands::{
            add_driver_to_special_location_zset, batch_check_drivers_on_ride,
            batch_get_driver_queue_last_ts, batch_get_driver_queue_trackings,
            push_drainer_driver_location, DriverQueueTracking,
        },
        keys::{
            driver_loc_bucket_key, driver_queue_last_ts_key, driver_queue_rank_history_key,
            driver_queue_tracking_key, special_location_queue_key,
        },
    },
};
use chrono::{DateTime, Utc};
use fred::prelude::{HashesInterface, KeysInterface, SortedSetsInterface};
use fred::types::{Expiration, GeoPosition, GeoValue, RedisValue, SetOptions};
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
use tokio::time::interval;
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
///
/// `queue_exit_hysteresis_threshold` is the number of consecutive out-of-geofence
/// pings required before a driver is actually removed from the queue. A value of
/// 1 reproduces the legacy "remove on first exit ping" behavior.
///
/// `queue_last_ts_ttl` is the **post-evict recovery window** (seconds) applied
/// to the last_ts key when a driver is hysteresis-evicted. While the driver is
/// actively pinging in-fence, last_ts is refreshed on every Enter at the full
/// `queue_expiry` so it never decays out from under a stationary driver. The
/// short TTL only kicks in at the moment of eviction: re-entry within this
/// window resumes the driver's original rank; longer absences (real exits)
/// let last_ts expire so the next Enter starts fresh.
/// TTL on the per-driver rank-history hash. 2h is a balance between letting
/// operators inspect rank churn over the recent past and keeping per-key
/// memory bounded for large fleets.
const RANK_HISTORY_TTL_SECS: i64 = 2 * 60 * 60;

/// Side data captured for each successful Enter so we can post-pipeline
/// ZRANK and HSET into the per-driver rank-history hash.
struct EnteredForRankHistory {
    merchant_id: String,
    driver_id: String,
    special_location_id: String,
    vehicle_type: String,
    timestamp: f64,
}

async fn drain_queue_actions(
    actions: Vec<QueueAction>,
    redis: &RedisConnectionPool,
    queue_expiry: u64,
    queue_exit_hysteresis_threshold: u32,
    queue_last_ts_ttl: u32,
) {
    let expiry_i64 = queue_expiry as i64;
    let last_ts_ttl_i64 = queue_last_ts_ttl as i64;

    // Per-batch side list of successful Enter events, used to record the
    // driver's resulting rank into a per-driver history hash after the main
    // pipeline executes. Order matches our follow-up ZRANK pipeline 1-to-1.
    let mut entered: Vec<EnteredForRankHistory> = Vec::new();

    // Step 1: Collect (merchant_id, driver_id) pairs for tracking + on_ride MGETs
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

    // Last-ts triples — the per-(queue, driver) earliest server timestamp we
    // use to stabilise rank against single-ping churn and cross-pod ordering.
    // PossibleExit/evict deletes happen via the pipeline in step 3 below;
    // those slots stay None so the helper skips them in the MGET while still
    // returning an actions-aligned vector for zipping.
    let last_ts_triples: Vec<Option<(&str, &str, &str)>> = actions
        .iter()
        .map(|a| match a {
            QueueAction::Enter {
                special_location_id,
                vehicle_type,
                driver_id,
                ..
            } => Some((
                special_location_id.as_str(),
                vehicle_type.as_str(),
                driver_id.as_str(),
            )),
            QueueAction::PossibleExit { .. } => None,
        })
        .collect();

    let (trackings, on_ride_flags, last_ts_results) = tokio::join!(
        batch_get_driver_queue_trackings(redis, &pairs),
        batch_check_drivers_on_ride(redis, &pairs),
        batch_get_driver_queue_last_ts(redis, &last_ts_triples)
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

    let last_ts_results = match last_ts_results {
        Ok(v) => v,
        Err(e) => {
            error!(tag = "[Queue Batch Last TS MGET]", error = %e);
            vec![None; actions.len()]
        }
    };

    // Step 2: Build all write commands into a single pipeline
    let pipeline = redis.writer_pool.next().pipeline();

    for (((action, old_tracking), is_on_ride), stored_last_ts) in actions
        .iter()
        .zip(trackings.iter())
        .zip(on_ride_flags.iter())
        .zip(last_ts_results.iter())
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
                    info!(tag = "[Queue Skip On Ride]", driver_id = %driver_id, special_location_id = %special_location_id, "Driver is on ride, skipping queue entry");
                    continue;
                }
                info!(tag = "[Queue Enter Processing]", driver_id = %driver_id, special_location_id = %special_location_id, vehicle_type = %vehicle_type, "Processing queue enter, old_tracking={:?}, stored_last_ts={:?}", old_tracking, stored_last_ts);
                let new_tracking = DriverQueueTracking {
                    special_location_id: special_location_id.clone(),
                    vehicle_type: vehicle_type.clone(),
                    // Reset the exit counter: an in-geofence ping cancels any
                    // in-progress hysteresis countdown.
                    consecutive_exit_pings: 0,
                };

                // If driver was in a different queue, evict from the old one:
                // both the queue ZSET and the per-queue last_ts of the old
                // (special_location_id, vehicle_type) become stale.
                if let Some(ref old) = old_tracking {
                    let same_queue = old.special_location_id == new_tracking.special_location_id
                        && old.vehicle_type == new_tracking.vehicle_type;
                    if !same_queue {
                        info!(
                            tag = "[Queue Switch Evict]",
                            driver_id = %driver_id,
                            merchant_id = %merchant_id,
                            old_special_location_id = %old.special_location_id,
                            old_vehicle_type = %old.vehicle_type,
                            old_consecutive_exit_pings = old.consecutive_exit_pings,
                            new_special_location_id = %special_location_id,
                            new_vehicle_type = %vehicle_type,
                            current_server_ts = *timestamp,
                            stored_last_ts = ?stored_last_ts,
                            "Driver entering different queue; evicting from old (ZREM old queue, DEL old last_ts)"
                        );
                        let old_key =
                            special_location_queue_key(&old.special_location_id, &old.vehicle_type);
                        let old_member = serde_json::to_string(driver_id.as_str())
                            .unwrap_or_else(|_| driver_id.clone());
                        let _ = pipeline
                            .zrem::<RedisValue, _, _>(&old_key, old_member)
                            .await;
                        let old_last_ts_key = driver_queue_last_ts_key(
                            &old.special_location_id,
                            &old.vehicle_type,
                            driver_id,
                        );
                        let _ = pipeline.del::<RedisValue, _>(&old_last_ts_key).await;
                        QUEUE_EVICTIONS
                            .with_label_values(&["switch", &old.special_location_id])
                            .inc();
                    }
                }

                let queue_key = special_location_queue_key(special_location_id, vehicle_type);
                let last_ts_key =
                    driver_queue_last_ts_key(special_location_id, vehicle_type, driver_id);
                let member =
                    serde_json::to_string(driver_id.as_str()).unwrap_or_else(|_| driver_id.clone());

                // Decide the score to use:
                //  - Some(stored) and current is earlier: lower the score so
                //    rank reflects the earliest-known arrival across pods.
                //  - Some(stored) otherwise: keep stored — covers same-driver
                //    repeat pings (NX no-op) and brief out-and-back-in within
                //    the TTL window (rank preserved).
                //  - None: first entry (or TTL elapsed since last ping) — use
                //    the current server_ts.
                let effective_score = match *stored_last_ts {
                    Some(stored) if *timestamp < stored => {
                        // ZREM + ZADD (no NX) so the lower score sticks.
                        let _ = pipeline
                            .zrem::<RedisValue, _, _>(&queue_key, member.as_str())
                            .await;
                        let _ = pipeline
                            .zadd::<RedisValue, _, _>(
                                &queue_key,
                                None,
                                None,
                                false,
                                false,
                                (*timestamp, member.as_str()),
                            )
                            .await;
                        *timestamp
                    }
                    Some(stored) => {
                        // ZADD NX: no-op if member is already present (the
                        // common case for a repeat ping); if the member was
                        // evicted (e.g. queue TTL refreshed differently from
                        // last_ts TTL), re-add at the original score.
                        let _ = pipeline
                            .zadd::<RedisValue, _, _>(
                                &queue_key,
                                Some(SetOptions::NX),
                                None,
                                false,
                                false,
                                (stored, member.as_str()),
                            )
                            .await;
                        stored
                    }
                    None => {
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
                        *timestamp
                    }
                };
                let _ = pipeline.expire::<(), _>(&queue_key, expiry_i64).await;

                // Refresh last_ts on every Enter at the full queue_expiry, so
                // a driver who keeps pinging in-fence never has their rank
                // memory decay. The post-evict short window is applied at the
                // hysteresis evict site below via EXPIRE.
                if let Ok(value) = serde_json::to_string(&effective_score) {
                    let _ = pipeline
                        .set::<RedisValue, _, _>(
                            &last_ts_key,
                            value,
                            Some(Expiration::EX(expiry_i64)),
                            None,
                            false,
                        )
                        .await;
                }

                // SET tracking key without expiry — only deleted on explicit queue exit.
                // This ensures we always know which queue a driver is in, even if
                // location updates stop temporarily.
                let tracking_key = driver_queue_tracking_key(merchant_id, driver_id);
                if let Ok(value) = serde_json::to_string(&new_tracking) {
                    let _ = pipeline
                        .set::<RedisValue, _, _>(&tracking_key, value, None, None, false)
                        .await;
                }

                entered.push(EnteredForRankHistory {
                    merchant_id: merchant_id.clone(),
                    driver_id: driver_id.clone(),
                    special_location_id: special_location_id.clone(),
                    vehicle_type: vehicle_type.clone(),
                    timestamp: *timestamp,
                });
            }
            QueueAction::PossibleExit {
                merchant_id,
                driver_id,
            } => {
                if let Some(ref tracking) = old_tracking {
                    // Hysteresis: require N consecutive exit pings before actually
                    // removing the driver. A single noisy GPS reading can put a
                    // driver outside the fence for one tick; if we ZREM on that
                    // single ping, the next in-fence ping re-adds them with a
                    // fresh timestamp and their rank jumps to the tail of the
                    // queue. Threshold of 1 disables hysteresis (legacy).
                    let threshold = queue_exit_hysteresis_threshold.max(1);
                    let next_count = tracking.consecutive_exit_pings.saturating_add(1);
                    let tracking_key = driver_queue_tracking_key(merchant_id, driver_id);

                    if next_count >= threshold {
                        info!(
                            tag = "[Queue Exit Evict]",
                            driver_id = %driver_id,
                            merchant_id = %merchant_id,
                            special_location_id = %tracking.special_location_id,
                            vehicle_type = %tracking.vehicle_type,
                            previous_exit_pings = tracking.consecutive_exit_pings,
                            next_exit_pings = next_count,
                            threshold = threshold,
                            queue_last_ts_ttl_sec = queue_last_ts_ttl,
                            "Hysteresis threshold reached: this PossibleExit ping bumped consecutive_exit_pings from previous to next, hitting threshold. ZREM driver from queue + DEL tracking. last_ts is intentionally NOT deleted — it lives until its TTL so a quick re-entry can still resume original rank, while a real exit (>> TTL) lets it expire naturally."
                        );
                        let queue_key = special_location_queue_key(
                            &tracking.special_location_id,
                            &tracking.vehicle_type,
                        );
                        let member = serde_json::to_string(driver_id.as_str())
                            .unwrap_or_else(|_| driver_id.clone());
                        let _ = pipeline.zrem::<RedisValue, _, _>(&queue_key, member).await;
                        let _ = pipeline.del::<RedisValue, _>(&tracking_key).await;
                        QUEUE_EVICTIONS
                            .with_label_values(&["hysteresis", &tracking.special_location_id])
                            .inc();
                        // Trim last_ts to the short recovery window. While
                        // the driver was in-fence, Enter refreshed last_ts at
                        // queue_expiry so it never decayed; here we cap the
                        // remaining lifetime so the TTL doubles as a
                        // false-evict-vs-real-exit discriminator:
                        //   - re-entry within `queue_last_ts_ttl` (e.g. GPS
                        //     noise, drainer race) → last_ts still alive →
                        //     ZADD NX with stored ts → original rank
                        //     restored.
                        //   - longer absence (real exit) → last_ts expires →
                        //     next Enter starts fresh at the tail.
                        let last_ts_key = driver_queue_last_ts_key(
                            &tracking.special_location_id,
                            &tracking.vehicle_type,
                            driver_id,
                        );
                        let _ = pipeline
                            .expire::<(), _>(&last_ts_key, last_ts_ttl_i64)
                            .await;
                    } else {
                        // Haven't hit threshold yet — keep the driver in the
                        // queue, just bump the counter in tracking. Preserves
                        // original timestamp / rank.
                        info!(
                            tag = "[Queue Exit Hysteresis]",
                            driver_id = %driver_id,
                            special_location_id = %tracking.special_location_id,
                            exit_pings = next_count,
                            threshold = threshold,
                            "Out-of-fence ping recorded; driver kept in queue"
                        );
                        let updated = DriverQueueTracking {
                            special_location_id: tracking.special_location_id.clone(),
                            vehicle_type: tracking.vehicle_type.clone(),
                            consecutive_exit_pings: next_count,
                        };
                        if let Ok(value) = serde_json::to_string(&updated) {
                            let _ = pipeline
                                .set::<RedisValue, _, _>(&tracking_key, value, None, None, false)
                                .await;
                        }
                    }
                } else {
                    info!(
                        tag = "[Queue Exit No-Op]",
                        driver_id = %driver_id,
                        "PossibleExit received but driver has no tracking; skipping"
                    );
                }
            }
        }
    }

    // Step 3: Execute entire pipeline in one round trip
    let result: Result<Vec<RedisValue>, _> = pipeline.all().await;
    if let Err(e) = result {
        error!(tag = "[Queue Pipeline Execute]", error = %e);
    }

    // Step 4: For every successful Enter, append (rank → timestamp) to that
    // driver's rank-history hash. Done after the main pipeline so ZRANK
    // observes the just-applied ZADD/score updates. This is observability,
    // not load-bearing state — failures are logged and swallowed.
    if !entered.is_empty() {
        record_rank_history(redis, &entered).await;
    }
}

/// Run two follow-up pipelines after the main drain:
///   1. Batched ZRANK for each Enter to discover the post-write rank.
///   2. Batched HSET (timestamp → rank) + EXPIRE on the per-driver hash.
async fn record_rank_history(redis: &RedisConnectionPool, entered: &[EnteredForRankHistory]) {
    let zrank_pipeline = redis.writer_pool.next().pipeline();
    for e in entered {
        let queue_key = special_location_queue_key(&e.special_location_id, &e.vehicle_type);
        let member =
            serde_json::to_string(e.driver_id.as_str()).unwrap_or_else(|_| e.driver_id.clone());
        let _ = zrank_pipeline
            .zrank::<RedisValue, _, _>(&queue_key, member)
            .await;
    }
    let ranks: Vec<RedisValue> = match zrank_pipeline.all().await {
        Ok(v) => v,
        Err(e) => {
            error!(tag = "[Queue Rank History ZRANK]", error = %e);
            return;
        }
    };

    let hset_pipeline = redis.writer_pool.next().pipeline();
    let mut queued = 0usize;
    for (e, rank_val) in entered.iter().zip(ranks.iter()) {
        // ZRANK returns nil if the member is not in the set (eviction raced
        // in, drainer reordering, etc.). Skip those — there's nothing useful
        // to record.
        let rank: u64 = match rank_val.clone().convert::<Option<u64>>() {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(err) => {
                error!(
                    tag = "[Queue Rank History Parse]",
                    driver_id = %e.driver_id,
                    error = %err,
                );
                continue;
            }
        };
        let key = driver_queue_rank_history_key(&e.merchant_id, &e.driver_id);
        let _ = hset_pipeline
            .hset::<RedisValue, _, _>(&key, (e.timestamp.to_string(), rank.to_string()))
            .await;
        let _ = hset_pipeline
            .expire::<(), _>(&key, RANK_HISTORY_TTL_SECS)
            .await;
        queued += 1;
    }
    if queued == 0 {
        return;
    }
    if let Err(e) = hset_pipeline.all::<Vec<RedisValue>>().await {
        error!(tag = "[Queue Rank History HSET]", error = %e);
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
#[allow(clippy::too_many_arguments)]
async fn drain_driver_locations(
    driver_locations: &DriversLocationMap,
    special_location_entries: &FxHashMap<String, Vec<(DriverId, u64, f64)>>,
    queue_actions: Vec<QueueAction>,
    bucket_expiry: i64,
    queue_expiry: u64,
    queue_exit_hysteresis_threshold: u32,
    entry_ts_ttl: u32,
    redis: &Arc<RedisConnectionPool>,
) {
    info!(
        tag = "[Queued Entries For Draining]",
        "Queue: {:?}\nPushing to redis server", driver_locations
    );

    if let Err(err) = push_drainer_driver_location(driver_locations, &bucket_expiry, redis).await {
        error!(tag = "[Error Pushing To Redis]", error = %err);
    }

    // Bucketed presence ZSET: only membership is consumed (by
    // `get_drivers_in_special_location`). Score has no semantic role beyond
    // ordering within a bucket, so a plain ZADD per ping is sufficient.
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
            drain_queue_actions(
                queue_actions,
                &redis,
                queue_expiry,
                queue_exit_hysteresis_threshold,
                entry_ts_ttl,
            )
            .await;
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
    mut rx: mpsc::Receiver<(
        Dimensions,
        Latitude,
        Longitude,
        TimeStamp,
        TimeStamp,
        DriverId,
    )>,
    graceful_termination_requested: Arc<AtomicBool>,
    drainer_capacity: usize,
    drainer_delay: u64,
    bucket_size: u64,
    near_by_bucket_threshold: u64,
    redis: Arc<RedisConnectionPool>,
    special_location_cache: Option<SpecialLocationCache>,
    enable_special_location_bucketing: bool,
    queue_expiry_seconds: u64,
    queue_exit_hysteresis_threshold: u32,
    enable_queue_cache_empty_guard: bool,
    special_location_entry_ts_ttl_sec: u64,
) {
    let mut driver_locations: DriversLocationMap = FxHashMap::default();
    let mut special_location_zset_entries: FxHashMap<String, Vec<(DriverId, u64, f64)>> =
        FxHashMap::default();
    let mut queue_actions: Vec<QueueAction> = Vec::new();
    let mut timer = interval(Duration::from_secs(drainer_delay));
    let mut drainer_queue_min_max_timestamp_range = None;

    let mut drainer_size = 0;

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
                    queue_exit_hysteresis_threshold,
                    special_location_entry_ts_ttl_sec as u32,
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
            break;
        }
        // TODO :: When drainer is ticked that time all locations coming to reciever could get dropped and vice versa.
        tokio::select! {
            item = rx.recv() => {
                info!(tag = "[Recieved Entries For Queuing]");
                match item {
                    Some((Dimensions { merchant_id, city, vehicle_type, created_at, merchant_operating_city_id }, Latitude(latitude), Longitude(longitude), TimeStamp(server_timestamp), TimeStamp(timestamp), DriverId(driver_id))) => {
                        let bucket = get_bucket_from_timestamp(&bucket_size, TimeStamp(timestamp));

                        let skip_normal_drain = if let Some(ref cache) = special_location_cache {
                            let guard = cache.read().await;
                            let city_entry_count = guard.get(&merchant_operating_city_id).map(|v| v.len()).unwrap_or(0);
                            info!(tag = "[Special Location Lookup]", driver_id = %driver_id, city_id = %merchant_operating_city_id.0, lat = %latitude, lon = %longitude, "Cache has {} locations for this city", city_entry_count);
                            if let Some(entry) = lookup_special_location(
                                &guard,
                                &merchant_operating_city_id,
                                &Latitude(latitude),
                                &Longitude(longitude),
                            ) {
                                info!(tag = "[Special Location Match]", driver_id = %driver_id, special_location_id = %entry.id.0, queue_enabled = %entry.is_queue_enabled, open_market = %entry.is_open_market_enabled);
                                if enable_special_location_bucketing {
                                    special_location_zset_entries
                                        .entry(entry.id.0.clone())
                                        .or_default()
                                        .push((
                                            DriverId(driver_id.clone()),
                                            bucket,
                                            server_timestamp.timestamp() as f64,
                                        ));
                                }
                                // Queue entry: if this special location is queue-enabled, enqueue driver
                                if entry.is_queue_enabled {
                                    info!(tag = "[Queue Action]", driver_id = %driver_id, special_location_id = %entry.id.0, vehicle_type = %vehicle_type, "Pushing Enter action");
                                    queue_actions.push(QueueAction::Enter {
                                        merchant_id: merchant_id.0.clone(),
                                        driver_id: driver_id.clone(),
                                        special_location_id: entry.id.0.clone(),
                                        vehicle_type: vehicle_type.to_string(),
                                        timestamp: server_timestamp.timestamp() as f64,
                                    });
                                }
                                !entry.is_open_market_enabled
                            } else if enable_queue_cache_empty_guard && city_entry_count == 0 {
                                // Cache has no polygons for this city (likely mid-reload or
                                // missing data). Treat as "unknown" instead of "outside" so we
                                // don't wipe the queue. Skip both PossibleExit and normal drain
                                // suppression.
                                info!(tag = "[Special Location Cache Empty Guard]", driver_id = %driver_id, city_id = %merchant_operating_city_id.0, "Skipping PossibleExit; cache has 0 entries for city");
                                false
                            } else {
                                // No special location match → possible exit from queue
                                info!(tag = "[Special Location No Match]", driver_id = %driver_id, city_id = %merchant_operating_city_id.0, lat = %latitude, lon = %longitude, "No geofence match, pushing PossibleExit");
                                queue_actions.push(QueueAction::PossibleExit {
                                    merchant_id: merchant_id.0.clone(),
                                    driver_id: driver_id.clone(),
                                });
                                false
                            }
                        } else {
                            info!(tag = "[Special Location Cache]", driver_id = %driver_id, "Cache is None (special_location_list_base_url not set)");
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

                        if drainer_size >= drainer_capacity {
                            info!(tag = "[Force Draining Queue]", length = %drainer_size);
                            let actions = std::mem::take(&mut queue_actions);
                            drain_driver_locations(
                                &driver_locations,
                                &special_location_zset_entries,
                                actions,
                                bucket_expiry,
                                queue_expiry_seconds,
                                queue_exit_hysteresis_threshold,
                                special_location_entry_ts_ttl_sec as u32,
                                &redis,
                            )
                            .await;
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
                    let actions = std::mem::take(&mut queue_actions);
                    drain_driver_locations(
                        &driver_locations,
                        &special_location_zset_entries,
                        actions,
                        bucket_expiry,
                        queue_expiry_seconds,
                        queue_exit_hysteresis_threshold,
                        special_location_entry_ts_ttl_sec as u32,
                        &redis,
                    )
                    .await;
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
