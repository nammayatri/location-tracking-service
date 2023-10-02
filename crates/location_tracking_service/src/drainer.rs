/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::{
    common::{types::*, utils::get_current_bucket},
    redis::keys::driver_loc_bucket_key,
};
use fred::{
    prelude::{GeoInterface, KeysInterface},
    types::{GeoPosition, GeoValue, RedisValue},
};
use shared::redis::types::RedisConnectionPool;
use shared::utils::{logger::*, prometheus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio::time::interval;

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
    let mut driver_locations = non_persistent_redis.pool.pipeline();
    let mut timer = interval(Duration::from_secs(drainer_delay));

    let mut new_ride_driver_locations = non_persistent_redis.pool.pipeline();
    let mut new_ride_timer = interval(Duration::from_secs(new_ride_drainer_delay));

    let mut drainer_size = 0;
    let mut new_ride_drainer_size = 0;

    let bucket_expiry = (bucket_size * near_by_bucket_threshold) as i64;

    loop {
        if graceful_termination_requested.load(Ordering::Relaxed) {
            info!(tag = "[Graceful Shutting Down]", length = %drainer_size);
            if drainer_size > 0 {
                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                let _ = driver_locations.all::<Vec<RedisValue>>().await;
                prometheus::QUEUE_COUNTER.reset();
            }
            if new_ride_drainer_size > 0 {
                info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                let _ = new_ride_driver_locations.all::<Vec<RedisValue>>().await;
                prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
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
                                let _ = new_ride_driver_locations.
                                    geoadd::<RedisValue, &str, GeoValue>(
                                        &driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket),
                                        None,
                                        false,
                                        GeoValue {
                                            coordinates: GeoPosition {
                                                latitude,
                                                longitude,
                                            },
                                            member: driver_id.into(),
                                        },
                                    )
                                    .await;
                                let _ = new_ride_driver_locations.expire::<(), &str>(&driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket), bucket_expiry).await;
                                new_ride_drainer_size += 1;
                                prometheus::NEW_RIDE_QUEUE_COUNTER.inc();
                            } else {
                                let _ = driver_locations.
                                    geoadd::<RedisValue, &str, GeoValue>(
                                        &driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket),
                                        None,
                                        false,
                                        GeoValue {
                                            coordinates: GeoPosition {
                                                latitude,
                                                longitude,
                                            },
                                            member: driver_id.into(),
                                        },
                                    )
                                    .await;
                                let _ = driver_locations.expire::<(), &str>(&driver_loc_bucket_key(&merchant_id, &city, &vehicle_type, &bucket), bucket_expiry).await;
                                drainer_size += 1;
                                prometheus::QUEUE_COUNTER.inc();
                            }
                            if drainer_size >= drainer_capacity {
                                info!(tag = "[Force Draining Queue]", length = %drainer_size);
                                let _ = driver_locations.all::<Vec<RedisValue>>().await;
                                driver_locations = non_persistent_redis.pool.pipeline();
                                // Cleanup
                                prometheus::QUEUE_COUNTER.reset();
                                drainer_size = 0;
                            }
                            if new_ride_drainer_size >= drainer_capacity {
                                info!(tag = "[Force Draining Queue - New Ride]", length = %new_ride_drainer_size);
                                let _ = new_ride_driver_locations.all::<Vec<RedisValue>>().await;
                                new_ride_driver_locations = non_persistent_redis.pool.pipeline();
                                // Cleanup
                                prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
                                new_ride_drainer_size = 0;
                            }
                        }
                    },
                    None => break,
                }
            },
            _ = timer.tick() => {
                if drainer_size > 0 {
                    info!(tag = "[Draining Queue]", length = %drainer_size);
                    let _ = driver_locations.all::<Vec<RedisValue>>().await;
                    driver_locations = non_persistent_redis.pool.pipeline();
                    // Cleanup
                    prometheus::QUEUE_COUNTER.reset();
                    drainer_size = 0;
                }
            },
            _ = new_ride_timer.tick() => {
                if new_ride_drainer_size > 0 {
                    info!(tag = "[Draining Queue - New Ride]", length = %new_ride_drainer_size);
                    let _ = new_ride_driver_locations.all::<Vec<RedisValue>>().await;
                    new_ride_driver_locations = non_persistent_redis.pool.pipeline();
                    // Cleanup
                    prometheus::NEW_RIDE_QUEUE_COUNTER.reset();
                    new_ride_drainer_size = 0;
                }
            },
        }
    }
}
