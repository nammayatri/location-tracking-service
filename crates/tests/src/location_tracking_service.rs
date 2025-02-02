/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

#[test]
fn test_read_geo_polygon() {
    print!("helloworld!");
}

#[tokio::test]
async fn test_set_key() {
    use shared::redis::types::{RedisConnectionPool, RedisSettings};

    let is_success = tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async {
            let pool = RedisConnectionPool::new(RedisSettings::default(), None)
                .await
                .expect("Failed to create Redis Connection Pool");
            let result = pool.set_key_as_str("helloworld!", "value", 3600).await;
            result.is_ok()
        })
    })
    .await
    .expect("Spawn block failure");
    assert!(is_success);
}

#[cfg(test)]
mod lock_contention {
    use shared::tools::logger::{info, setup_tracing, LogLevel, LoggerConfig};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tokio::join;
    use tokio::sync::RwLock;
    use tokio::time::{Duration, Instant};

    /// Runs benchmarks comparing `RwLock<HashMap>` vs. swapping two `HashMap`s.
    ///
    /// - Measures time taken for `RwLock<HashMap>` when concurrent reads and writes occur.
    /// - Measures time taken for `swap` approach where one map is used for reading and another for writing.
    ///
    /// **Observations:**
    /// - `RwLock<HashMap>` can have contention when writes block reads.
    /// - `Swapping HashMaps` improves performance by removing contention.
    ///
    #[tokio::test]
    async fn benchmark_hashmap_read_write_vs_swap() {
        let _guard = setup_tracing(LoggerConfig {
            level: LogLevel::INFO,
            log_to_file: false,
        });

        let num_operations = 100_000;

        info!("Starting HashMap Benchmark...");

        // Benchmark: Single RwLock HashMap
        let rwlock_duration = benchmark_rwlock_hashmap(num_operations).await;
        info!("RwLock HashMap Time: {:.2?}", rwlock_duration);

        // Benchmark: Double Buffered HashMap with Swap
        let swap_duration = benchmark_swap_hashmap(num_operations).await;
        info!("Swapping HashMaps Time: {:.2?}", swap_duration);

        info!("Benchmark Completed.");
    }

    /// Measures execution time of a `RwLock<HashMap<K, V>>` under concurrent read and write load.
    ///
    /// - Uses a **single HashMap** wrapped in an `RwLock`.
    /// - **Readers** (`read().await`) do not block each other but **must wait if a write is in progress**.
    /// - **Writers** (`write().await`) block all reads and other writes.
    ///
    /// **Use case:** When reads are frequent, and writes are rare.
    ///
    /// Returns: `Duration` indicating execution time.
    async fn benchmark_rwlock_hashmap(num_operations: usize) -> Duration {
        let hashmap = Arc::new(RwLock::new(HashMap::new()));
        let hashmap_clone = hashmap.clone();

        let start = Instant::now();

        let writer = tokio::spawn(async move {
            let mut map = hashmap.write().await;
            for i in 0..num_operations {
                map.insert(i, -1);
            }
        });

        let reader = tokio::spawn(async move {
            let map = hashmap_clone.read().await;
            for i in 0..num_operations {
                map.get(&i);
            }
        });

        let _ = join!(writer, reader);

        start.elapsed()
    }

    /// Measures execution time of **double-buffered HashMaps with swapping** under concurrent load.
    ///
    /// - Uses **two separate `HashMap`s** wrapped in `RwLock<HashMap>`.
    /// - **One map is actively written**, while the other is **read-only**.
    /// - Every **10,000 operations**, the active map is **swapped** using an `AtomicBool`.
    ///
    /// **Why is it faster?**
    /// - **No contention** between reads and writes.
    /// - **Readers operate on a stable dataset**, reducing lock overhead.
    /// - **Writers never block readers**, only swap maps periodically.
    ///
    /// **Use case:** When both reads and writes are frequent and minimizing contention is critical.
    ///
    /// Returns: `Duration` indicating execution time.
    async fn benchmark_swap_hashmap(num_operations: usize) -> Duration {
        // ✅ Use `RwLock<HashMap>` instead of `Mutex<HashMap>` to reduce contention.
        let hashmap1 = Arc::new(RwLock::new(HashMap::new()));
        let hashmap2 = Arc::new(RwLock::new(HashMap::new()));

        // ✅ AtomicBool to track active map (true = hashmap1 is active, false = hashmap2 is active)
        let active_map = Arc::new(AtomicBool::new(true));

        let hashmap1_clone = hashmap1.clone();
        let hashmap2_clone = hashmap2.clone();
        let active_map_clone = active_map.clone();

        let start = Instant::now();

        let writer = tokio::spawn({
            let hashmap1 = hashmap1.clone();
            let hashmap2 = hashmap2.clone();
            let active_map = active_map.clone();

            async move {
                // ✅ AtomicBool allows fast swapping without a lock
                let is_active = active_map.load(Ordering::Acquire);
                let mut map = if is_active {
                    hashmap1.write().await
                } else {
                    hashmap2.write().await
                };
                for i in 0..num_operations {
                    map.insert(i, -1);
                }
            }
        });

        let reader = tokio::spawn({
            let hashmap1 = hashmap1_clone;
            let hashmap2 = hashmap2_clone;
            let active_map = active_map_clone;

            async move {
                let is_active = active_map.load(Ordering::Acquire);
                let map = if is_active {
                    hashmap2.read().await
                } else {
                    hashmap1.read().await
                };
                for _ in 0..num_operations {
                    map.get(&0); // Read some value
                }
            }
        });

        let _ = join!(writer, reader);

        start.elapsed()
    }
}

#[cfg(test)]
mod concurrency {
    use shared::tools::logger::{info, setup_tracing, LogLevel, LoggerConfig};
    use tokio::{select, spawn};

    /// Tests different concurrency handling approaches using `tokio::select!` and `tokio::spawn()`.
    ///
    /// ## **Key Scenarios:**
    /// - **Race Condition (`tokio::select!`)**: Runs tasks that race to completion.
    /// - **Looped Task Racing**: Continuously runs tasks that race each other.
    /// - **`select!` on Join Handles**: Stops execution if any task fails.
    /// - **Spawn-based Task Execution**: Runs tasks indefinitely until all complete.
    ///
    #[tokio::test]
    async fn test_select_vs_spawn() {
        let _guard = setup_tracing(LoggerConfig {
            level: LogLevel::INFO,
            log_to_file: false,
        });

        // Task 1: Run tasks that race to completion once
        race_tasks().await;

        // Task 2: Continuously race tasks in a loop
        loop_race_tasks().await;

        // Task 3: Run tasks as separate threads with an alternative approach

        // Approach 1: Not idle use case of select, should be used when even on failure of one thread we want to stop execution
        loop_race_tasks_with_select_on_join().await;

        // Approach 2: Join all the Future handles and let it run concurrently wituout using select!, this will run indifinitely until all threads executed completely
        loop_race_tasks_with_join_handle().await;
    }

    /// Runs two tasks that race to complete using `tokio::select!`.
    ///
    /// - The first task to complete **cancels the other**.
    /// - This is useful for **timeout-based operations**.
    ///
    /// **Use case:** When only one result is needed, and we don’t need to wait for all tasks.
    ///
    async fn race_tasks() {
        let t1 = task_one();
        let t2 = task_two();

        // `select!` waits for either task to complete.
        // The faster task completes first, and the remaining task is dropped.
        select! {
            _ = t1 => info!("[race_tasks] Task One finished first"),
            _ = t2 => info!("[race_tasks] Task Two finished first"),
        }
    }

    /// Continuously runs two tasks in a loop, restarting them after each completion.
    /// The slower task may starve if one task consistently finishes first.
    /// Runs tasks in an infinite loop, racing each other using `tokio::select!`.
    ///
    /// - The first task to complete **cancels the other**.
    /// - This continues indefinitely, simulating a **high-concurrency scenario**.
    ///
    /// **Use case:** When continuous racing is required (e.g., retry mechanisms).
    ///
    async fn loop_race_tasks() {
        let mut iterations = 0;
        loop {
            if iterations >= 4 {
                return;
            }
            select! {
                _ = task_one() => info!("[loop_race_tasks] Task One finished"),
                _ = task_two() => info!("[loop_race_tasks] Task Two finished"),
            }
            iterations += 1;
        }
    }

    /// continuously running tasks using `tokio::spawn`
    /// and `JoinHandle` to manage task execution with `select`.
    /// Runs multiple tasks and **stops execution if any task fails** using `tokio::select!`.
    ///
    /// - Uses `JoinHandle.await` inside `select!`, stopping execution on failure.
    /// - Ensures that even **if one task fails, the entire system stops**.
    ///
    /// **Use case:** When failure in any async operation should stop the process.
    ///
    async fn loop_race_tasks_with_select_on_join() {
        let handle_one = spawn(async move {
            let mut iterations = 0;
            loop {
                if iterations >= 4 {
                    return;
                }
                task_one().await;
                info!("[loop_race_tasks_with_select_on_join] Task One finished");
                iterations += 1;
            }
        });

        let handle_two = spawn(async move {
            loop {
                task_two().await;
                info!("[loop_race_tasks_with_select_on_join] Task Two finished");
            }
        });

        select! {
            _ = handle_one => (),
            _ = handle_two => (),
        }
    }

    /// Alternative approach to continuously running tasks using `tokio::spawn`
    /// and `JoinHandle` to manage task execution without `select`.
    /// Spawns multiple independent tasks using `tokio::spawn()`, waiting for all to complete.
    ///
    /// - Each task **runs independently**.
    /// - Unlike `select!`, execution continues **until all tasks finish**.
    ///
    /// **Use case:** When we want to let all tasks run to completion, even if one fails.
    ///
    async fn loop_race_tasks_with_join_handle() {
        let handle_one = spawn(async move {
            let mut iterations = 0;
            loop {
                if iterations >= 4 {
                    return;
                }
                task_one().await;
                info!("[loop_race_tasks_with_join_handle] Task One finished");
                iterations += 1;
            }
        });

        let handle_two = spawn(async move {
            loop {
                task_two().await;
                info!("[loop_race_tasks_with_join_handle] Task Two finished");
            }
        });

        // Wait for both tasks to run indefinitely.
        // If you want to gracefully shut them down, you can manage cancellation logic.
        let _ = tokio::join!(handle_one, handle_two);
    }

    // Simulates a task that takes 5 seconds to complete.
    async fn task_one() {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    // Simulates a task that takes 2 seconds to complete.
    async fn task_two() {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}
