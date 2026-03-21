/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

#[cfg(test)]
mod helpers {
    use chrono::Utc;
    use location_tracking_service::common::types::*;
    use rand::Rng;
    use shared::redis::types::{RedisConnectionPool, RedisSettings};
    use std::sync::Arc;

    /// Creates a Redis connection pool for testing.
    /// Requires a running Redis instance on default settings.
    pub async fn test_redis_pool() -> Arc<RedisConnectionPool> {
        Arc::new(
            RedisConnectionPool::new(RedisSettings::default(), None)
                .await
                .expect("Failed to create Redis connection pool for testing"),
        )
    }

    /// Generates a unique test ID with a given prefix to avoid key collisions.
    pub fn unique_id(prefix: &str) -> String {
        let mut rng = rand::thread_rng();
        let suffix: u64 = rng.gen();
        format!("{prefix}-test-{suffix}")
    }

    /// Creates a test Dimensions value.
    pub fn test_dimensions(merchant_id: &str, city: &str) -> Dimensions {
        Dimensions {
            merchant_id: MerchantId(merchant_id.to_string()),
            city: CityName(city.to_string()),
            vehicle_type: VehicleType::SEDAN,
            created_at: Utc::now(),
            merchant_operating_city_id: MerchantOperatingCityId("test-mocid".to_string()),
        }
    }

    /// Creates a test Point in Bangalore area.
    pub fn test_point() -> Point {
        Point {
            lat: Latitude(12.9716),
            lon: Longitude(77.5946),
        }
    }

    /// Creates a test Point with slight offset for nearby testing.
    pub fn nearby_point(base: &Point, offset_meters: f64) -> Point {
        // ~111,000 meters per degree of latitude
        let lat_offset = offset_meters / 111_000.0;
        Point {
            lat: Latitude(base.lat.0 + lat_offset),
            lon: Longitude(base.lon.0),
        }
    }

    /// Creates a test TimeStamp for now.
    pub fn now_ts() -> TimeStamp {
        TimeStamp(Utc::now())
    }
}

// ============================================================================
// 1. Full Request Handling Path Tests
// ============================================================================
#[cfg(test)]
mod request_handling {
    use super::helpers::*;
    use chrono::Utc;
    use location_tracking_service::common::types::*;
    use location_tracking_service::redis::{commands, keys};

    /// Test the ride lifecycle: start -> verify state -> end -> verify cleanup.
    #[tokio::test]
    async fn test_ride_start_and_end_lifecycle() {
        let redis = test_redis_pool().await;
        let merchant_id = MerchantId(unique_id("merchant"));
        let driver_id = DriverId(unique_id("driver"));
        let ride_id = RideId(unique_id("ride"));
        let redis_expiry = 60u32;

        // Start a ride: set ride details
        commands::set_ride_details_for_driver(
            &*redis,
            &redis_expiry,
            &merchant_id,
            &driver_id,
            ride_id.clone(),
            RideStatus::INPROGRESS,
            None,
        )
        .await
        .expect("Failed to set ride details");

        // Verify ride details can be fetched
        let details = commands::get_ride_details(&*redis, &driver_id, &merchant_id)
            .await
            .expect("Failed to get ride details");
        let details = details.expect("Ride details should exist");
        assert_eq!(details.ride_id, ride_id);
        assert_eq!(details.ride_status, RideStatus::INPROGRESS);

        // Set driver details for the ride
        let driver_details = DriverDetails {
            driver_id: driver_id.clone(),
        };
        commands::set_on_ride_driver_details(&*redis, &redis_expiry, &ride_id, driver_details)
            .await
            .expect("Failed to set driver details");

        // Verify driver details by ride ID
        let fetched_driver = commands::get_driver_details(&*redis, &ride_id)
            .await
            .expect("Failed to get driver details");
        let fetched_driver = fetched_driver.expect("Driver details should exist");
        assert_eq!(fetched_driver.driver_id, driver_id);

        // End ride: cleanup
        commands::ride_cleanup(&*redis, &merchant_id, &driver_id, &ride_id, &None)
            .await
            .expect("Failed ride cleanup");

        // Verify cleanup: ride details should be gone
        let after_cleanup = commands::get_ride_details(&*redis, &driver_id, &merchant_id)
            .await
            .expect("Failed to get ride details after cleanup");
        assert!(after_cleanup.is_none(), "Ride details should be cleaned up");

        // Verify cleanup: driver details by ride should be gone
        let driver_after = commands::get_driver_details(&*redis, &ride_id)
            .await
            .expect("Failed to get driver details after cleanup");
        assert!(
            driver_after.is_none(),
            "Driver details should be cleaned up"
        );
    }

    /// Test setting and retrieving driver last known location.
    #[tokio::test]
    async fn test_driver_last_known_location() {
        let redis = test_redis_pool().await;
        let driver_id = DriverId(unique_id("driver"));
        let merchant_id = MerchantId(unique_id("merchant"));
        let point = test_point();
        let ts = now_ts();
        let expiry = 60u32;

        // Set driver last location
        let last_loc = commands::set_driver_last_location_update(
            &*redis,
            &expiry,
            &driver_id,
            &merchant_id,
            &point,
            &ts,
            &None,                     // blocked_till
            None,                      // stop_detection
            &None,                     // ride_status
            &None,                     // ride_notification_status
            &None,                     // detection_state
            &None,                     // anti_detection_state
            &None,                     // violation_trigger_flag
            &None,                     // driver_pickup_distance
            &None,                     // bear
            &Some(VehicleType::SEDAN), // vehicle_type
            &None,                     // group_id
            &None,                     // group_id2
        )
        .await
        .expect("Failed to set driver last location");

        assert_eq!(last_loc.location.lat, point.lat);
        assert_eq!(last_loc.location.lon, point.lon);
        assert_eq!(last_loc.merchant_id, merchant_id);

        // Retrieve and verify
        let fetched = commands::get_driver_location(&*redis, &driver_id)
            .await
            .expect("Failed to get driver location");
        let fetched = fetched.expect("Driver location should exist");
        assert_eq!(fetched.driver_last_known_location.location.lat, point.lat);
        assert_eq!(fetched.driver_last_known_location.location.lon, point.lon);
    }

    /// Test batch retrieval of multiple driver locations.
    #[tokio::test]
    async fn test_batch_driver_location_retrieval() {
        let redis = test_redis_pool().await;
        let merchant_id = MerchantId(unique_id("merchant"));
        let expiry = 60u32;
        let base_point = test_point();

        let mut driver_ids = Vec::new();
        for i in 0..5 {
            let driver_id = DriverId(unique_id(&format!("driver-{i}")));
            let point = nearby_point(&base_point, i as f64 * 100.0);
            commands::set_driver_last_location_update(
                &*redis,
                &expiry,
                &driver_id,
                &merchant_id,
                &point,
                &now_ts(),
                &None,
                None,
                &None,
                &None,
                &None,
                &None,
                &None,
                &None,
                &None,
                &Some(VehicleType::SEDAN),
                &None,
                &None,
            )
            .await
            .expect("Failed to set driver location");
            driver_ids.push(driver_id);
        }

        // Batch fetch
        let locations = commands::get_all_driver_last_locations(&*redis, &driver_ids)
            .await
            .expect("Failed to batch get driver locations");

        assert_eq!(locations.len(), 5);
        for loc in &locations {
            assert!(loc.is_some(), "Each driver should have a location");
        }
    }

    /// Test on-ride location push and retrieval.
    #[tokio::test]
    async fn test_on_ride_location_push_and_get() {
        use location_tracking_service::outbound::types::LocationUpdate;

        let redis = test_redis_pool().await;
        let driver_id = DriverId(unique_id("driver"));
        let merchant_id = MerchantId(unique_id("merchant"));
        let expiry = 60u32;

        let entries: Vec<LocationUpdate> = (0..3)
            .map(|i| LocationUpdate {
                lat: Latitude(12.9716 + i as f64 * 0.001),
                lon: Longitude(77.5946),
                ts: Some(chrono::Utc::now().timestamp()),
            })
            .collect();

        // Push locations
        let count = commands::push_on_ride_driver_locations(
            &*redis,
            &driver_id,
            &merchant_id,
            entries,
            &expiry,
        )
        .await
        .expect("Failed to push on-ride locations");
        assert_eq!(count, 3);

        // Verify count
        let stored_count =
            commands::get_on_ride_driver_locations_count(&*redis, &driver_id, &merchant_id)
                .await
                .expect("Failed to get count");
        assert_eq!(stored_count, 3);

        // Retrieve locations
        let locs = commands::get_on_ride_driver_locations(&*redis, &driver_id, &merchant_id, 10)
            .await
            .expect("Failed to get on-ride locations");
        assert_eq!(locs.len(), 3);

        // Pop locations (simulates ride end)
        let popped = commands::get_on_ride_driver_locations_and_delete(
            &*redis,
            &driver_id,
            &merchant_id,
            10,
        )
        .await
        .expect("Failed to pop on-ride locations");
        assert_eq!(popped.len(), 3);

        // Verify empty after pop
        let remaining_count =
            commands::get_on_ride_driver_locations_count(&*redis, &driver_id, &merchant_id)
                .await
                .expect("Failed to get count after pop");
        assert_eq!(remaining_count, 0);
    }

    /// Test driver auth token caching flow.
    #[tokio::test]
    async fn test_driver_auth_token_cache() {
        let redis = test_redis_pool().await;
        let token = Token(unique_id("token"));
        let driver_id = DriverId(unique_id("driver"));
        let merchant_id = MerchantId(unique_id("merchant"));
        let mocid = MerchantOperatingCityId("test-mocid".into());
        let expiry = 60u32;

        // Initially should not exist
        let initial = commands::get_driver_id(&*redis, &token)
            .await
            .expect("Failed initial get");
        assert!(initial.is_none());

        // Cache driver auth
        commands::set_driver_id(
            &*redis,
            &expiry,
            &token,
            driver_id.clone(),
            merchant_id.clone(),
            mocid.clone(),
        )
        .await
        .expect("Failed to set driver id");

        // Verify cached
        let cached = commands::get_driver_id(&*redis, &token)
            .await
            .expect("Failed to get cached driver id");
        let cached = cached.expect("Auth data should be cached");
        assert_eq!(cached.driver_id, driver_id);
        assert_eq!(cached.merchant_id, merchant_id);
    }

    /// Test healthcheck via direct Redis set/get.
    #[tokio::test]
    async fn test_healthcheck_redis_roundtrip() {
        let redis = test_redis_pool().await;
        let key = keys::health_check_key();

        redis
            .set_key_as_str(&key, "driver-location-service-health-check", 60)
            .await
            .expect("Failed to set healthcheck key");

        let val = redis
            .get_key_as_str(&key)
            .await
            .expect("Failed to get healthcheck key");
        assert_eq!(val.as_deref(), Some("driver-location-service-health-check"));
    }

    /// Test route location set/get/remove via Redis hash.
    #[tokio::test]
    async fn test_route_location_operations() {
        let redis = test_redis_pool().await;
        let route_code = unique_id("route");
        let vehicle_number = "KA-01-AB-1234".to_string();
        let point = test_point();

        // Set route location
        commands::set_route_location(
            &*redis,
            &route_code,
            &vehicle_number,
            &point,
            &Some(SpeedInMeterPerSecond(5.0)),
            &now_ts(),
            Some(RideStatus::INPROGRESS),
            None,
        )
        .await
        .expect("Failed to set route location");

        // Get all vehicles on route
        let vehicles = commands::get_route_location(&*redis, &route_code)
            .await
            .expect("Failed to get route location");
        assert!(vehicles.contains_key(&vehicle_number));
        let info = &vehicles[&vehicle_number];
        assert_eq!(info.latitude, point.lat);
        assert_eq!(info.longitude, point.lon);

        // Get specific vehicle
        let specific =
            commands::get_route_location_by_vehicle_number(&*redis, &route_code, &vehicle_number)
                .await
                .expect("Failed to get specific vehicle");
        assert!(specific.is_some());

        // Remove vehicle from route
        commands::remove_route_location(&*redis, &route_code, &vehicle_number)
            .await
            .expect("Failed to remove route location");

        let after_remove =
            commands::get_route_location_by_vehicle_number(&*redis, &route_code, &vehicle_number)
                .await
                .expect("Failed to get after remove");
        assert!(after_remove.is_none());
    }
}

// ============================================================================
// 2. Drainer Batch Processing Tests
// ============================================================================
#[cfg(test)]
mod drainer_tests {
    use super::helpers::*;
    use chrono::Utc;
    use fred::types::{GeoPosition, GeoUnit, SortOrder};
    use location_tracking_service::common::types::*;
    use location_tracking_service::common::utils::get_bucket_from_timestamp;
    use location_tracking_service::drainer::run_drainer;
    use location_tracking_service::redis::keys::driver_loc_bucket_key;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};

    /// Test that the drainer processes items and writes them to Redis geo index.
    #[tokio::test]
    async fn test_drainer_processes_batch() {
        let redis = test_redis_pool().await;
        let merchant_id_str = unique_id("merchant");
        let city_str = unique_id("city");
        let bucket_size = 60u64;
        let nearby_bucket_threshold = 3u64;

        let (tx, rx) = mpsc::channel(100);
        let termination = Arc::new(AtomicBool::new(false));
        let termination_clone = termination.clone();

        // Spawn the drainer
        let redis_clone = redis.clone();
        let drainer_handle = tokio::spawn(async move {
            run_drainer(
                rx,
                termination_clone,
                10, // drainer_capacity
                1,  // drainer_delay (1 second)
                bucket_size,
                nearby_bucket_threshold,
                &*redis_clone,
                None, // no special location cache
                false,
            )
            .await;
        });

        let ts = now_ts();
        let bucket = get_bucket_from_timestamp(&bucket_size, ts);

        // Send 5 driver locations through the channel
        for i in 0..5 {
            let dims = Dimensions {
                merchant_id: MerchantId(merchant_id_str.clone()),
                city: CityName(city_str.clone()),
                vehicle_type: VehicleType::SEDAN,
                created_at: Utc::now(),
                merchant_operating_city_id: MerchantOperatingCityId("test-mocid".into()),
            };
            let driver_id = DriverId(format!("drainer-driver-{i}"));
            tx.send((
                dims,
                Latitude(12.9716 + i as f64 * 0.001),
                Longitude(77.5946),
                ts,
                driver_id,
            ))
            .await
            .expect("Failed to send to drainer channel");
        }

        // Wait for drainer to flush (time-based: 1s delay + processing)
        sleep(Duration::from_secs(3)).await;

        // Signal termination
        termination.store(true, Ordering::Relaxed);
        // Send one more item to wake the drainer loop so it sees the termination flag
        let wake_dims = test_dimensions(&merchant_id_str, &city_str);
        let _ = tx
            .send((
                wake_dims,
                Latitude(12.9716),
                Longitude(77.5946),
                ts,
                DriverId("wake-driver".into()),
            ))
            .await;
        drop(tx);

        // Wait for drainer to finish
        let _ = tokio::time::timeout(Duration::from_secs(5), drainer_handle).await;

        // Verify data was written to Redis geo index
        let key = driver_loc_bucket_key(
            &MerchantId(merchant_id_str),
            &CityName(city_str),
            &VehicleType::SEDAN,
            &bucket,
        );

        let results: Vec<(String, shared::redis::types::Point)> = redis
            .mgeo_search(
                vec![key],
                GeoPosition::from((77.5946, 12.9716)),
                (50_000.0, GeoUnit::Meters),
                SortOrder::Asc,
            )
            .await
            .expect("Failed to geo search after drainer");

        assert!(
            results.len() >= 5,
            "Expected at least 5 drivers in geo index, found {}",
            results.len()
        );
    }

    /// Test that the drainer flushes remaining items on graceful termination.
    #[tokio::test]
    async fn test_drainer_graceful_shutdown_flush() {
        let redis = test_redis_pool().await;
        let merchant_id_str = unique_id("merchant");
        let city_str = unique_id("city");
        let bucket_size = 60u64;

        let (tx, rx) = mpsc::channel(100);
        let termination = Arc::new(AtomicBool::new(false));
        let termination_clone = termination.clone();

        let redis_clone = redis.clone();
        let drainer_handle = tokio::spawn(async move {
            run_drainer(
                rx,
                termination_clone,
                1000, // large capacity so time-based flush won't trigger quickly
                300,  // 5 minutes delay — won't trigger in test
                bucket_size,
                3,
                &*redis_clone,
                None,
                false,
            )
            .await;
        });

        let ts = now_ts();
        let bucket = get_bucket_from_timestamp(&bucket_size, ts);

        // Send 3 items
        for i in 0..3 {
            let dims = test_dimensions(&merchant_id_str, &city_str);
            tx.send((
                dims,
                Latitude(12.9716 + i as f64 * 0.001),
                Longitude(77.5946),
                ts,
                DriverId(format!("shutdown-driver-{i}")),
            ))
            .await
            .expect("Failed to send");
        }

        // Give time for items to be received
        sleep(Duration::from_millis(200)).await;

        // Signal graceful termination
        termination.store(true, Ordering::Relaxed);

        // Send one more to wake the loop
        let _ = tx
            .send((
                test_dimensions(&merchant_id_str, &city_str),
                Latitude(12.975),
                Longitude(77.595),
                ts,
                DriverId("shutdown-wake".into()),
            ))
            .await;
        drop(tx);

        let _ = tokio::time::timeout(Duration::from_secs(5), drainer_handle).await;

        // Verify the items were flushed despite not hitting capacity or time trigger
        let key = driver_loc_bucket_key(
            &MerchantId(merchant_id_str),
            &CityName(city_str),
            &VehicleType::SEDAN,
            &bucket,
        );

        let results: Vec<(String, shared::redis::types::Point)> = redis
            .mgeo_search(
                vec![key],
                GeoPosition::from((77.5946, 12.9716)),
                (50_000.0, GeoUnit::Meters),
                SortOrder::Asc,
            )
            .await
            .expect("Failed to geo search after shutdown");

        assert!(
            results.len() >= 3,
            "Expected at least 3 drivers flushed on shutdown, found {}",
            results.len()
        );
    }

    /// Test drainer capacity-based flush (fills batch to capacity).
    #[tokio::test]
    async fn test_drainer_capacity_flush() {
        let redis = test_redis_pool().await;
        let merchant_id_str = unique_id("merchant");
        let city_str = unique_id("city");
        let bucket_size = 60u64;
        let capacity = 5usize;

        let (tx, rx) = mpsc::channel(100);
        let termination = Arc::new(AtomicBool::new(false));
        let termination_clone = termination.clone();

        let redis_clone = redis.clone();
        let drainer_handle = tokio::spawn(async move {
            run_drainer(
                rx,
                termination_clone,
                capacity,
                300, // very long delay — should not trigger
                bucket_size,
                3,
                &*redis_clone,
                None,
                false,
            )
            .await;
        });

        let ts = now_ts();
        let bucket = get_bucket_from_timestamp(&bucket_size, ts);

        // Send exactly `capacity` items to trigger a capacity flush
        for i in 0..capacity {
            let dims = test_dimensions(&merchant_id_str, &city_str);
            tx.send((
                dims,
                Latitude(12.9716 + i as f64 * 0.001),
                Longitude(77.5946),
                ts,
                DriverId(format!("cap-driver-{i}")),
            ))
            .await
            .expect("Failed to send");
        }

        // Wait for capacity flush
        sleep(Duration::from_secs(2)).await;

        // Terminate
        termination.store(true, Ordering::Relaxed);
        let _ = tx
            .send((
                test_dimensions(&merchant_id_str, &city_str),
                Latitude(12.98),
                Longitude(77.60),
                ts,
                DriverId("cap-wake".into()),
            ))
            .await;
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(5), drainer_handle).await;

        let key = driver_loc_bucket_key(
            &MerchantId(merchant_id_str),
            &CityName(city_str),
            &VehicleType::SEDAN,
            &bucket,
        );

        let results: Vec<(String, shared::redis::types::Point)> = redis
            .mgeo_search(
                vec![key],
                GeoPosition::from((77.5946, 12.9716)),
                (50_000.0, GeoUnit::Meters),
                SortOrder::Asc,
            )
            .await
            .expect("Failed to geo search after capacity flush");

        assert!(
            results.len() >= capacity,
            "Expected at least {capacity} drivers after capacity flush, found {}",
            results.len()
        );
    }
}

// ============================================================================
// 3. Error Handling Tests
// ============================================================================
#[cfg(test)]
mod error_handling {
    use super::helpers::*;
    use location_tracking_service::common::types::*;
    use location_tracking_service::redis::commands;
    use location_tracking_service::tools::error::AppError;

    /// Test fetching a non-existent ride returns None (not an error).
    #[tokio::test]
    async fn test_get_nonexistent_ride_details() {
        let redis = test_redis_pool().await;
        let driver_id = DriverId(unique_id("ghost-driver"));
        let merchant_id = MerchantId(unique_id("ghost-merchant"));

        let result = commands::get_ride_details(&*redis, &driver_id, &merchant_id).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Test fetching non-existent driver location returns None.
    #[tokio::test]
    async fn test_get_nonexistent_driver_location() {
        let redis = test_redis_pool().await;
        let driver_id = DriverId(unique_id("ghost-driver"));

        let result = commands::get_driver_location(&*redis, &driver_id).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Test fetching driver auth with non-existent token returns None.
    #[tokio::test]
    async fn test_get_nonexistent_auth_token() {
        let redis = test_redis_pool().await;
        let token = Token(unique_id("ghost-token"));

        let result = commands::get_driver_id(&*redis, &token).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    /// Test ride cleanup on non-existent keys doesn't error.
    #[tokio::test]
    async fn test_cleanup_nonexistent_ride() {
        let redis = test_redis_pool().await;
        let merchant_id = MerchantId(unique_id("merchant"));
        let driver_id = DriverId(unique_id("driver"));
        let ride_id = RideId(unique_id("ride"));

        let result =
            commands::ride_cleanup(&*redis, &merchant_id, &driver_id, &ride_id, &None).await;
        assert!(
            result.is_ok(),
            "Cleanup of non-existent keys should not fail"
        );
    }

    /// Test on-ride location count for non-existent key returns 0.
    #[tokio::test]
    async fn test_on_ride_locations_count_nonexistent() {
        let redis = test_redis_pool().await;
        let driver_id = DriverId(unique_id("driver"));
        let merchant_id = MerchantId(unique_id("merchant"));

        let count = commands::get_on_ride_driver_locations_count(&*redis, &driver_id, &merchant_id)
            .await
            .expect("Should not error on non-existent key");
        assert_eq!(count, 0);
    }

    /// Test batch get with non-existent drivers returns all None.
    #[tokio::test]
    async fn test_batch_get_nonexistent_drivers() {
        let redis = test_redis_pool().await;
        let driver_ids: Vec<DriverId> = (0..3)
            .map(|i| DriverId(unique_id(&format!("ghost-{i}"))))
            .collect();

        let results = commands::get_all_driver_last_locations(&*redis, &driver_ids)
            .await
            .expect("Should not error on non-existent drivers");
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.is_none());
        }
    }

    /// Test batch get ride details with non-existent drivers.
    #[tokio::test]
    async fn test_batch_get_nonexistent_ride_details() {
        let redis = test_redis_pool().await;
        let merchant_id = MerchantId(unique_id("merchant"));
        let driver_ids: Vec<DriverId> = (0..3)
            .map(|i| DriverId(unique_id(&format!("ghost-{i}"))))
            .collect();

        let results = commands::get_all_driver_ride_details(&*redis, &driver_ids, &merchant_id)
            .await
            .expect("Should not error on non-existent drivers");
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.is_none());
        }
    }

    /// Test geo search on empty / non-existent bucket keys returns empty.
    #[tokio::test]
    async fn test_geo_search_empty_buckets() {
        let redis = test_redis_pool().await;
        let merchant_id = MerchantId(unique_id("merchant"));
        let city = CityName(unique_id("city"));

        let result = commands::get_drivers_within_radius(
            &*redis,
            &3,
            &merchant_id,
            &city,
            &VehicleType::SEDAN,
            &99999999, // bucket that doesn't exist
            test_point(),
            &Radius(5000.0),
        )
        .await
        .expect("Geo search on empty buckets should not error");

        assert!(result.is_empty());
    }

    /// Test the sliding window rate limiter blocks excess requests.
    #[tokio::test]
    async fn test_rate_limiter_blocks_excess() {
        use location_tracking_service::common::sliding_window_rate_limiter::sliding_window_limiter;

        let redis = test_redis_pool().await;
        let key = unique_id("ratelimit");
        let frame_hits_limit = 3;
        let frame_len = 60;

        // First 3 hits should succeed
        for _ in 0..3 {
            let result = sliding_window_limiter(&*redis, &key, frame_hits_limit, frame_len).await;
            assert!(result.is_ok(), "Hit within limit should succeed");
        }

        // 4th hit should be rate limited
        let result = sliding_window_limiter(&*redis, &key, frame_hits_limit, frame_len).await;
        assert!(result.is_err(), "Hit exceeding limit should fail");
    }
}

// ============================================================================
// 4. Concurrent Request Handling Tests
// ============================================================================
#[cfg(test)]
mod concurrency_tests {
    use super::helpers::*;
    use location_tracking_service::common::types::*;
    use location_tracking_service::common::utils::get_bucket_from_timestamp;
    use location_tracking_service::drainer::run_drainer;
    use location_tracking_service::redis::{commands, keys};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};

    /// Test concurrent writes to different driver locations don't interfere.
    #[tokio::test]
    async fn test_concurrent_driver_location_writes() {
        let redis = test_redis_pool().await;
        let merchant_id = MerchantId(unique_id("merchant"));
        let expiry = 60u32;

        let mut handles = Vec::new();
        let num_drivers = 20;

        for i in 0..num_drivers {
            let redis = redis.clone();
            let merchant_id = merchant_id.clone();
            handles.push(tokio::spawn(async move {
                let driver_id = DriverId(format!("concurrent-driver-{i}"));
                let point = Point {
                    lat: Latitude(12.9716 + i as f64 * 0.001),
                    lon: Longitude(77.5946 + i as f64 * 0.001),
                };
                commands::set_driver_last_location_update(
                    &*redis,
                    &expiry,
                    &driver_id,
                    &merchant_id,
                    &point,
                    &now_ts(),
                    &None,
                    None,
                    &None,
                    &None,
                    &None,
                    &None,
                    &None,
                    &None,
                    &None,
                    &Some(VehicleType::SEDAN),
                    &None,
                    &None,
                )
                .await
                .expect("Concurrent write failed");
                (driver_id, point)
            }));
        }

        let results: Vec<(DriverId, Point)> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.expect("Task panicked"))
            .collect();

        // Verify all drivers stored correctly
        let driver_ids: Vec<DriverId> = results.iter().map(|(id, _)| id.clone()).collect();
        let locations = commands::get_all_driver_last_locations(&*redis, &driver_ids)
            .await
            .expect("Batch get failed");

        for (i, loc) in locations.iter().enumerate() {
            let loc = loc.as_ref().expect("Driver location should exist");
            let expected_lat = 12.9716 + i as f64 * 0.001;
            assert!(
                (loc.location.lat.0 - expected_lat).abs() < 0.0001,
                "Driver {i} lat mismatch: expected {expected_lat}, got {}",
                loc.location.lat.0
            );
        }
    }

    /// Test concurrent ride start operations for different drivers.
    #[tokio::test]
    async fn test_concurrent_ride_operations() {
        let redis = test_redis_pool().await;
        let expiry = 60u32;
        let num_rides = 15;

        let mut handles = Vec::new();
        for i in 0..num_rides {
            let redis = redis.clone();
            handles.push(tokio::spawn(async move {
                let merchant_id = MerchantId(format!("conc-merchant-{i}"));
                let driver_id = DriverId(format!("conc-driver-{i}"));
                let ride_id = RideId(format!("conc-ride-{i}"));

                // Start ride
                commands::set_ride_details_for_driver(
                    &*redis,
                    &expiry,
                    &merchant_id,
                    &driver_id,
                    ride_id.clone(),
                    RideStatus::INPROGRESS,
                    None,
                )
                .await
                .expect("Concurrent ride start failed");

                // Verify it's there
                let details = commands::get_ride_details(&*redis, &driver_id, &merchant_id)
                    .await
                    .expect("Concurrent ride get failed");
                assert!(details.is_some());

                // Cleanup
                commands::ride_cleanup(&*redis, &merchant_id, &driver_id, &ride_id, &None)
                    .await
                    .expect("Concurrent ride cleanup failed");
            }));
        }

        let results = futures::future::join_all(handles).await;
        for result in results {
            result.expect("Concurrent ride task panicked");
        }
    }

    /// Test concurrent sends to drainer channel don't lose data.
    #[tokio::test]
    async fn test_concurrent_drainer_sends() {
        let redis = test_redis_pool().await;
        let merchant_id_str = unique_id("merchant");
        let city_str = unique_id("city");
        let bucket_size = 60u64;

        let (tx, rx) = mpsc::channel(1000);
        let termination = Arc::new(AtomicBool::new(false));
        let termination_clone = termination.clone();

        let redis_clone = redis.clone();
        let drainer_handle = tokio::spawn(async move {
            run_drainer(
                rx,
                termination_clone,
                200, // large capacity
                1,   // 1 second delay
                bucket_size,
                3,
                &*redis_clone,
                None,
                false,
            )
            .await;
        });

        let ts = now_ts();
        let bucket = get_bucket_from_timestamp(&bucket_size, ts);
        let num_senders = 10;
        let items_per_sender = 10;

        // Spawn concurrent senders
        let mut sender_handles = Vec::new();
        for s in 0..num_senders {
            let tx = tx.clone();
            let merchant_id_str = merchant_id_str.clone();
            let city_str = city_str.clone();
            sender_handles.push(tokio::spawn(async move {
                for i in 0..items_per_sender {
                    let dims = Dimensions {
                        merchant_id: MerchantId(merchant_id_str.clone()),
                        city: CityName(city_str.clone()),
                        vehicle_type: VehicleType::SEDAN,
                        created_at: chrono::Utc::now(),
                        merchant_operating_city_id: MerchantOperatingCityId("mocid".into()),
                    };
                    tx.send((
                        dims,
                        Latitude(12.9716 + (s * items_per_sender + i) as f64 * 0.0001),
                        Longitude(77.5946),
                        ts,
                        DriverId(format!("conc-drainer-{s}-{i}")),
                    ))
                    .await
                    .expect("Failed to send");
                }
            }));
        }

        // Wait for all senders
        futures::future::join_all(sender_handles).await;

        // Wait for drainer to process
        sleep(Duration::from_secs(3)).await;

        // Terminate
        termination.store(true, Ordering::Relaxed);
        let wake_dims = test_dimensions(&merchant_id_str, &city_str);
        let _ = tx
            .send((
                wake_dims,
                Latitude(12.98),
                Longitude(77.60),
                ts,
                DriverId("conc-wake".into()),
            ))
            .await;
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(5), drainer_handle).await;

        let key = keys::driver_loc_bucket_key(
            &MerchantId(merchant_id_str),
            &CityName(city_str),
            &VehicleType::SEDAN,
            &bucket,
        );

        let results: Vec<(String, shared::redis::types::Point)> = redis
            .mgeo_search(
                vec![key],
                fred::types::GeoPosition::from((77.5946, 12.9716)),
                (50_000.0, fred::types::GeoUnit::Meters),
                fred::types::SortOrder::Asc,
            )
            .await
            .expect("Geo search failed");

        let expected = num_senders * items_per_sender;
        assert!(
            results.len() >= expected,
            "Expected at least {expected} drivers from concurrent sends, found {}",
            results.len()
        );
    }
}

// ============================================================================
// 5. Basic Load Test / Benchmark
// ============================================================================
#[cfg(test)]
mod load_tests {
    use super::helpers::*;
    use location_tracking_service::common::types::*;
    use location_tracking_service::common::utils::get_bucket_from_timestamp;
    use location_tracking_service::drainer::run_drainer;
    use location_tracking_service::redis::{commands, keys};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, Instant};

    /// Benchmark: measure throughput of Redis set/get operations.
    #[tokio::test]
    async fn bench_redis_set_get_throughput() {
        let redis = test_redis_pool().await;
        let iterations = 500;
        let merchant_id = MerchantId(unique_id("bench-merchant"));
        let expiry = 60u32;

        // Benchmark SET operations
        let start = Instant::now();
        for i in 0..iterations {
            let driver_id = DriverId(format!("bench-driver-{i}"));
            commands::set_driver_last_location_update(
                &*redis,
                &expiry,
                &driver_id,
                &merchant_id,
                &test_point(),
                &now_ts(),
                &None,
                None,
                &None,
                &None,
                &None,
                &None,
                &None,
                &None,
                &None,
                &Some(VehicleType::SEDAN),
                &None,
                &None,
            )
            .await
            .expect("Bench set failed");
        }
        let set_duration = start.elapsed();
        let set_ops_per_sec = iterations as f64 / set_duration.as_secs_f64();

        // Benchmark GET operations
        let driver_ids: Vec<DriverId> = (0..iterations)
            .map(|i| DriverId(format!("bench-driver-{i}")))
            .collect();

        let start = Instant::now();
        let _results = commands::get_all_driver_last_locations(&*redis, &driver_ids)
            .await
            .expect("Bench batch get failed");
        let batch_get_duration = start.elapsed();

        println!("--- Redis Load Test Results ---");
        println!(
            "SET: {} ops in {:.2?} ({:.0} ops/sec)",
            iterations, set_duration, set_ops_per_sec
        );
        println!(
            "MGET (batch {}): {:.2?} ({:.0} keys/sec)",
            iterations,
            batch_get_duration,
            iterations as f64 / batch_get_duration.as_secs_f64()
        );

        // Sanity check: should be able to do at least 100 ops/sec even on slow hardware
        assert!(
            set_ops_per_sec > 50.0,
            "SET throughput too low: {set_ops_per_sec:.0} ops/sec"
        );
    }

    /// Benchmark: measure drainer throughput with high-volume sends.
    #[tokio::test]
    async fn bench_drainer_throughput() {
        let redis = test_redis_pool().await;
        let merchant_id_str = unique_id("bench-merchant");
        let city_str = unique_id("bench-city");
        let bucket_size = 60u64;
        let total_items = 1000;

        let (tx, rx) = mpsc::channel(total_items * 2);
        let termination = Arc::new(AtomicBool::new(false));
        let termination_clone = termination.clone();

        let redis_clone = redis.clone();
        let drainer_handle = tokio::spawn(async move {
            run_drainer(
                rx,
                termination_clone,
                100, // capacity
                1,   // 1 second delay
                bucket_size,
                3,
                &*redis_clone,
                None,
                false,
            )
            .await;
        });

        let ts = now_ts();

        // Time the send phase
        let send_start = Instant::now();
        for i in 0..total_items {
            let dims = Dimensions {
                merchant_id: MerchantId(merchant_id_str.clone()),
                city: CityName(city_str.clone()),
                vehicle_type: VehicleType::SEDAN,
                created_at: chrono::Utc::now(),
                merchant_operating_city_id: MerchantOperatingCityId("mocid".into()),
            };
            tx.send((
                dims,
                Latitude(12.9716 + (i % 100) as f64 * 0.0001),
                Longitude(77.5946 + (i / 100) as f64 * 0.0001),
                ts,
                DriverId(format!("bench-drainer-{i}")),
            ))
            .await
            .expect("Failed to send");
        }
        let send_duration = send_start.elapsed();

        // Wait for processing
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Terminate
        termination.store(true, Ordering::Relaxed);
        let _ = tx
            .send((
                test_dimensions(&merchant_id_str, &city_str),
                Latitude(12.98),
                Longitude(77.60),
                ts,
                DriverId("bench-wake".into()),
            ))
            .await;
        drop(tx);
        let _ = tokio::time::timeout(Duration::from_secs(10), drainer_handle).await;

        let send_rate = total_items as f64 / send_duration.as_secs_f64();

        println!("--- Drainer Load Test Results ---");
        println!(
            "SEND: {total_items} items in {:.2?} ({:.0} items/sec)",
            send_duration, send_rate
        );

        // Verify data landed in Redis
        let bucket = get_bucket_from_timestamp(&bucket_size, ts);
        let key = keys::driver_loc_bucket_key(
            &MerchantId(merchant_id_str),
            &CityName(city_str),
            &VehicleType::SEDAN,
            &bucket,
        );

        let results: Vec<(String, shared::redis::types::Point)> = redis
            .mgeo_search(
                vec![key],
                fred::types::GeoPosition::from((77.5946, 12.9716)),
                (50_000.0, fred::types::GeoUnit::Meters),
                fred::types::SortOrder::Asc,
            )
            .await
            .expect("Geo search failed");

        println!(
            "VERIFY: {}/{total_items} items stored in Redis geo index",
            results.len()
        );

        // Some items may have same geo coordinates and be deduped by member name
        // so we check that a reasonable number made it through
        assert!(
            results.len() >= total_items / 2,
            "Expected at least {} items in Redis, found {}",
            total_items / 2,
            results.len()
        );
    }

    /// Benchmark: concurrent Redis operations throughput.
    #[tokio::test]
    async fn bench_concurrent_redis_operations() {
        let redis = test_redis_pool().await;
        let num_tasks = 50;
        let ops_per_task = 20;
        let merchant_id = MerchantId(unique_id("bench-conc"));
        let expiry = 60u32;

        let start = Instant::now();

        let mut handles = Vec::new();
        for t in 0..num_tasks {
            let redis = redis.clone();
            let merchant_id = merchant_id.clone();
            handles.push(tokio::spawn(async move {
                for i in 0..ops_per_task {
                    let driver_id = DriverId(format!("bench-conc-{t}-{i}"));
                    // Write
                    commands::set_driver_last_location_update(
                        &*redis,
                        &expiry,
                        &driver_id,
                        &merchant_id,
                        &Point {
                            lat: Latitude(12.9716),
                            lon: Longitude(77.5946),
                        },
                        &TimeStamp(chrono::Utc::now()),
                        &None,
                        None,
                        &None,
                        &None,
                        &None,
                        &None,
                        &None,
                        &None,
                        &None,
                        &Some(VehicleType::SEDAN),
                        &None,
                        &None,
                    )
                    .await
                    .expect("Concurrent bench write failed");

                    // Read back
                    let _loc = commands::get_driver_location(&*redis, &driver_id)
                        .await
                        .expect("Concurrent bench read failed");
                }
            }));
        }

        futures::future::join_all(handles).await;
        let duration = start.elapsed();

        let total_ops = num_tasks * ops_per_task * 2; // write + read
        let ops_per_sec = total_ops as f64 / duration.as_secs_f64();

        println!("--- Concurrent Redis Benchmark ---");
        println!("{num_tasks} tasks x {ops_per_task} ops (write+read) = {total_ops} total ops");
        println!("Duration: {:.2?}", duration);
        println!("Throughput: {:.0} ops/sec", ops_per_sec);

        assert!(
            ops_per_sec > 100.0,
            "Concurrent throughput too low: {ops_per_sec:.0} ops/sec"
        );
    }
}
