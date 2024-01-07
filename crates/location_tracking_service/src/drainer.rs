/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::queue_drainer_latency;
use crate::tools::prometheus::{NEW_RIDE_QUEUE_COUNTER, QUEUE_COUNTER, QUEUE_DRAINER_LATENCY};
use crate::{
    common::{types::*, utils::get_bucket_from_timestamp},
    redis::{commands::push_drainer_driver_location, keys::driver_loc_bucket_key},
};
use chrono::{DateTime, Utc};
use fred::types::{GeoPosition, GeoValue};
use rustc_hash::FxHashMap;
use shared::redis::types::RedisConnectionPool;
use std::{
    cmp::min,
    sync::atomic::{AtomicBool, Ordering},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{error, info};

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
/// * `non_persistent_redis` - A reference to the Redis connection pool.
///
/// # Example
///
/// This function is typically used within the context of `run_drainer`:
///
/// ```ignore
/// async fn run_drainer(...) {
///     ...
///     drain_driver_locations(&driver_locations, bucket_expiry, &non_persistent_redis).await;
///     ...
/// }
/// ```
async fn drain_driver_locations(
    driver_locations: &FxHashMap<String, Vec<GeoValue>>,
    bucket_expiry: i64,
    non_persistent_redis: &RedisConnectionPool,
) {
    info!(
        tag = "[Queued Entries For Draining]",
        "Queue: {:?}\nPushing to redis server", driver_locations
    );

    if let Err(err) =
        push_drainer_driver_location(driver_locations, &bucket_expiry, non_persistent_redis).await
    {
        error!(tag = "[Error Pushing To Redis]", error = %err);
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
/// * `start_time` - A mutable reference to the start time of the current data batch.
/// * `tag` - A string representing the type of ride (`"OFF_RIDE"` or `"NEW_RIDE"`).
///
fn cleanup_drainer(
    drainer_size: &mut usize,
    driver_locations: &mut FxHashMap<String, Vec<GeoValue>>,
    start_time: &mut DateTime<Utc>,
    tag: &str,
) {
    queue_drainer_latency!(tag, *start_time);
    *start_time = Utc::now();
    if tag == "OFF_RIDE" {
        QUEUE_COUNTER.reset()
    } else {
        NEW_RIDE_QUEUE_COUNTER.reset();
    }
    *drainer_size = 0;
    *driver_locations = FxHashMap::default();
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
/// * `new_ride_drainer_delay` - Time interval for draining new ride data.
/// * `bucket_size` - The size of each time bucket.
/// * `near_by_bucket_threshold` - A threshold for nearby buckets.
/// * `non_persistent_redis` - Redis connection pool for non-persistent storage.
///
#[allow(clippy::too_many_arguments)]
pub async fn run_drainer(
    mut rx: mpsc::Receiver<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    graceful_termination_requested: Arc<AtomicBool>,
    drainer_capacity: usize,
    drainer_delay: u64,
    new_ride_drainer_delay: u64,
    bucket_size: u64,
    near_by_bucket_threshold: u64,
    non_persistent_redis: &RedisConnectionPool,
) {
    let mut driver_locations: FxHashMap<String, Vec<GeoValue>> = FxHashMap::default();
    let mut timer = interval(Duration::from_secs(drainer_delay));
    let mut start_time = Utc::now();

    let mut new_ride_driver_locations: FxHashMap<String, Vec<GeoValue>> = FxHashMap::default();
    let mut new_ride_timer = interval(Duration::from_secs(new_ride_drainer_delay));
    let mut new_ride_start_time = Utc::now();

    let mut drainer_size = 0;
    let mut new_ride_drainer_size = 0;

    let bucket_expiry = (bucket_size * near_by_bucket_threshold) as i64;

    loop {
        if graceful_termination_requested.load(Ordering::Relaxed) {
            info!(tag = "[Graceful Shutting Down]", length = %drainer_size);
            if drainer_size > 0 {
                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                drain_driver_locations(&driver_locations, bucket_expiry, non_persistent_redis)
                    .await;
                cleanup_drainer(
                    &mut drainer_size,
                    &mut driver_locations,
                    &mut start_time,
                    "OFF_RIDE",
                );
            }
            if new_ride_drainer_size > 0 {
                info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                drain_driver_locations(
                    &new_ride_driver_locations,
                    bucket_expiry,
                    non_persistent_redis,
                )
                .await;
                cleanup_drainer(
                    &mut new_ride_drainer_size,
                    &mut new_ride_driver_locations,
                    &mut new_ride_start_time,
                    "NEW_RIDE",
                );
            }
            break;
        }
        tokio::select! {
            item = rx.recv() => {
                info!(tag = "[Recieved Entries For Queuing]");
                match item {
                    Some((Dimensions { merchant_id, city, vehicle_type, new_ride }, Latitude(latitude), Longitude(longitude), TimeStamp(timestamp), DriverId(driver_id))) => {
                        let bucket = get_bucket_from_timestamp(&bucket_size, TimeStamp(timestamp));
                        if new_ride {
                            new_ride_driver_locations
                                .entry(driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket))
                                .or_default()
                                .push(GeoValue {
                                    coordinates: GeoPosition {
                                        latitude,
                                        longitude,
                                    },
                                    member: driver_id.into(),
                                });
                            new_ride_start_time = min(start_time, timestamp);
                            new_ride_drainer_size += 1;
                            NEW_RIDE_QUEUE_COUNTER.inc();
                        } else {
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
                            start_time = min(start_time, timestamp);
                            drainer_size += 1;
                            QUEUE_COUNTER.inc();
                        }
                        if drainer_size >= drainer_capacity {
                            info!(tag = "[Force Draining Queue]", length = %drainer_size);
                            drain_driver_locations(&driver_locations, bucket_expiry, non_persistent_redis).await;
                            cleanup_drainer(
                                &mut drainer_size,
                                &mut driver_locations,
                                &mut start_time,
                                "OFF_RIDE",
                            );
                        }
                        if new_ride_drainer_size >= drainer_capacity {
                            info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                            drain_driver_locations(&new_ride_driver_locations, bucket_expiry, non_persistent_redis).await;
                            cleanup_drainer(
                                &mut new_ride_drainer_size,
                                &mut new_ride_driver_locations,
                                &mut new_ride_start_time,
                                "NEW_RIDE",
                            );
                        }
                    },
                    None => break,
                }
            },
            _ = timer.tick() => {
                if drainer_size > 0 {
                    info!(tag = "[Draining Queue]", length = %drainer_size);
                    drain_driver_locations(&driver_locations, bucket_expiry, non_persistent_redis).await;
                    cleanup_drainer(
                        &mut drainer_size,
                        &mut driver_locations,
                        &mut start_time,
                        "OFF_RIDE",
                    );
                }
            },
            _ = new_ride_timer.tick() => {
                if new_ride_drainer_size > 0 {
                    info!(tag = "[Draining Queue - New Ride]", length = %new_ride_drainer_size);
                    drain_driver_locations(&new_ride_driver_locations, bucket_expiry, non_persistent_redis).await;
                    cleanup_drainer(
                        &mut new_ride_drainer_size,
                        &mut new_ride_driver_locations,
                        &mut new_ride_start_time,
                        "NEW_RIDE",
                    );
                }
            },
        }
    }
}
