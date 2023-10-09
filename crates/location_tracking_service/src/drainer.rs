/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::{
    common::{types::*, utils::get_current_bucket},
    redis::{commands::push_drainer_driver_location, keys::driver_loc_bucket_key},
};
use fred::types::{GeoPosition, GeoValue};
use rustc_hash::FxHashMap;
use shared::redis::types::RedisConnectionPool;
use shared::utils::{logger::*, prometheus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio::time::interval;

async fn drain_driver_locations(
    driver_locations: &FxHashMap<String, Vec<GeoValue>>,
    bucket_expiry: i64,
    non_persistent_redis: &RedisConnectionPool,
) {
    info!(
        tag = "[Queued Entries For Draining]",
        "Queue: {:?}\nPushing to redis server", driver_locations
    );

    let res =
        push_drainer_driver_location(driver_locations, &bucket_expiry, non_persistent_redis).await;

    if let Err(err) = res {
        error!(tag = "[Error Pushing To Redis]", error = %err);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_drainer(
    mut rx: mpsc::Receiver<(Dimensions, Latitude, Longitude, DriverId)>,
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

    let mut new_ride_driver_locations: FxHashMap<String, Vec<GeoValue>> = FxHashMap::default();
    let mut new_ride_timer = interval(Duration::from_secs(new_ride_drainer_delay));

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
                prometheus::QUEUE_COUNTER.reset();
                driver_locations.clear();
            }
            if new_ride_drainer_size > 0 {
                info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                drain_driver_locations(
                    &new_ride_driver_locations,
                    bucket_expiry,
                    non_persistent_redis,
                )
                .await;
                prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
                driver_locations.clear();
            }
            break;
        }
        tokio::select! {
            item = rx.recv() => {
                info!(tag = "[Recieved Entries For Queuing]");
                match item {
                    Some((Dimensions { merchant_id, city, vehicle_type, new_ride }, Latitude(latitude), Longitude(longitude), DriverId(driver_id))) => {
                        if let Ok(bucket) = get_current_bucket(&bucket_size) {
                            if new_ride {
                                new_ride_driver_locations
                                    .entry(driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket))
                                    .or_insert_with(Vec::new)
                                    .push(GeoValue {
                                        coordinates: GeoPosition {
                                            latitude,
                                            longitude,
                                        },
                                        member: driver_id.into(),
                                    });
                                new_ride_drainer_size += 1;
                                prometheus::NEW_RIDE_QUEUE_COUNTER.inc();
                            } else {
                                driver_locations
                                    .entry(driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket))
                                    .or_insert_with(Vec::new)
                                    .push(GeoValue {
                                        coordinates: GeoPosition {
                                            latitude,
                                            longitude,
                                        },
                                        member: driver_id.into(),
                                    });
                                drainer_size += 1;
                                prometheus::QUEUE_COUNTER.inc();
                            }
                            if drainer_size >= drainer_capacity {
                                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                                drain_driver_locations(&driver_locations, bucket_expiry, non_persistent_redis).await;
                                // Cleanup
                                prometheus::QUEUE_COUNTER.reset();
                                drainer_size = 0;
                                driver_locations.clear();
                            }
                            if new_ride_drainer_size >= drainer_capacity {
                                info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                                drain_driver_locations(&new_ride_driver_locations, bucket_expiry, non_persistent_redis).await;
                                // Cleanup
                                prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
                                new_ride_drainer_size = 0;
                                new_ride_driver_locations.clear();
                            }
                        }
                    },
                    None => break,
                }
            },
            _ = timer.tick() => {
                if drainer_size > 0 {
                    info!(tag = "[Draining Queue]", length = %drainer_size);
                    drain_driver_locations(&driver_locations, bucket_expiry, non_persistent_redis).await;
                    // Cleanup
                    prometheus::QUEUE_COUNTER.reset();
                    drainer_size = 0;
                    driver_locations.clear();
                }
            },
            _ = new_ride_timer.tick() => {
                if new_ride_drainer_size > 0 {
                    info!(tag = "[Draining Queue - New Ride]", length = %new_ride_drainer_size);
                    drain_driver_locations(&new_ride_driver_locations, bucket_expiry, non_persistent_redis).await;
                    // Cleanup
                    prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
                    new_ride_drainer_size = 0;
                    new_ride_driver_locations.clear();
                }
            },
        }
    }
}
