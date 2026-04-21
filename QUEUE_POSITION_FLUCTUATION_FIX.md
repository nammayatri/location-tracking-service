# Queue Position Fluctuation — Root Cause & Fixes

## The bug

Drivers calling the queue-position API saw their rank jump wildly (e.g. `17-20` → `35-38`) and, sometimes, rank returned `null` even though the driver was "still in the queue". No one was manually adding or removing drivers.

## How the queue is built (recap)

- Key: `lts:special_loc_queue:{special_location_id}:{vehicle_type}` — a Redis **ZSET**.
- Member: JSON-encoded `driver_id`.
- Score: Unix timestamp of first entry.
- Position returned by the API: `ZRANK` (+1 for 1-indexing), with a `queue_position_range_offset` window around it.
- Queue add: `ZADD NX` from the drainer when a driver's location ping falls inside a queue-enabled geofence.
- Queue remove: `ZREM` when the drainer sees a location ping *outside* any special-location geofence, triggered as `QueueAction::PossibleExit`.

Key insight: because of `ZADD NX`, a driver's score (and therefore rank) is **stable** as long as they remain in the ZSET. Rank can only worsen if the driver is removed and then re-added with a fresher timestamp.

## Root causes

### Cause 1 — Single noisy GPS ping evicts the driver *(primary cause of rank jumps)*

Location: `crates/location_tracking_service/src/drainer.rs`, the `QueueAction::PossibleExit` path.

Every location ping ran a geofence lookup. A *single* ping outside the polygon produced a `PossibleExit`, which immediately did `ZREM` + `DEL tracking_key`. On the driver's next in-fence ping, they were re-added via `ZADD NX` with the current timestamp — placing them at the **tail** of the queue.

This is why rank jumped from `17` to `35`: the driver wasn't just "pushed back"; they were evicted and re-queued behind everyone whose original score was older than *now*.

GPS noise near polygon edges, or inside polygons with jagged boundaries, made this happen without any real movement.

### Cause 2 — Special-location cache briefly empty wipes queues

Location: `drainer.rs` again, same path.

The in-memory `special_location_cache` is populated by an async fetch. During initial load, a reload, or a transient miss for a city, `lookup_special_location` returns `None` → the drainer emits `PossibleExit` for every live driver in that city → the whole queue is flushed in one tick. When pings start matching again, everyone re-enters at the tail, re-ordered by whichever ping arrived first. Ranks scramble.

### Cause 3 — Non-atomic `ZCARD` + `ZRANK`

Location: `domain/action/internal/location.rs::driver_queue_position`.

The API computed `queue_position_range` as `[rank ± offset]` and returned `queue_size` alongside it. `ZRANK` and `ZCARD` were two separate Redis calls. With concurrent writes, the two values could disagree, producing ranges like `(17, 100)` with a size of `18`.

### Cause 4 — Queue-wide churn causes bidirectional rank fluctuation **and** intermittent `null`

> Note on deployment: in our current setup there is **no Redis replica**. `reader_pool` and `writer_pool` in shared-kernel's `RedisConnectionPool` are both instantiated against the same Redis config, so both talk to the same (primary) node. That means "replica lag" is *not* a live cause for us — but queue-wide churn from Cause 1 produces the same visible symptoms, explained below. If a read replica is ever added later, the same symptoms would re-emerge from genuine replica lag, so we still switched the position read to `writer_pool` as a defensive measure.

With the single-ping eviction bug, evictions aren't happening just to the target driver — they're happening to **every** driver in the queue. A driver's rank is simply the count of ZSET members with a smaller score, so the target's rank swings as other members get `ZREM`-ed and re-`ZADD`-ed around them.

Worked example for the `17 → 35 → 17` pattern observed, single-node Redis, same driver X queried repeatedly:

- **T1**: X at rank 17 — 17 drivers ahead of X.
- **T1 → T2**: 18 new drivers join behind X. X still at rank 17.
- **T2**: X gets a single noisy out-of-fence ping → `ZREM` + `ZADD` with a fresh timestamp → X now sits behind the 17 originals **plus** the 18 new joiners → rank = **35**.
- **T2 → T3**: those 18 drivers who joined between T1 and T2 each hit the same single-ping bug and get re-queued with even fresher timestamps, landing behind X → X's rank drops back to **17**.
- During any tiny window where X is `ZREM`-ed but the re-`ZADD` hasn't happened yet, `ZRANK` returns `nil` → API returns `queue_position_range = null`.

So the oscillation *and* the `null` are two faces of the same bug (Cause 1), not a separate read-path problem on single-node Redis. The reason the rank comes *back down* is that the churn is happening queue-wide, not just to X; members ahead of X keep getting re-queued to the tail, dropping X's rank.

The clean fix for both symptoms is therefore Fix 1 (hysteresis) — stop the churn at the source. Fix 4 is defensive only.

### Non-causes we ruled out

- **Member encoding mismatch** — every write (drainer, manual_add, force_add) and every read (rank, zrem) uses `serde_json::to_string(driver_id)`. Consistent.
- **`is_on_ride` flip** — the drainer only `continue`s on `Enter` when on-ride; it doesn't `ZREM`. So this alone doesn't evict.
- **TTL expiry** — `queue_expiry_seconds` defaults to 86400 (1 day), refreshed on every `Enter`. Can only bite if a driver gets zero in-fence pings for 24h.

## The fixes

### Fix 1 — Exit hysteresis on `PossibleExit`

Changed files: `redis/commands.rs`, `drainer.rs`.

`DriverQueueTracking` now carries a `consecutive_exit_pings: u32` counter (with `#[serde(default)]` so existing stored values deserialize cleanly). On each `PossibleExit`:

- Increment the counter in tracking.
- Only when the counter reaches the configured threshold do we `ZREM` + `DEL` tracking.
- Below threshold, we **keep the driver in the queue** and only update tracking with the new counter.

On each `Enter`, we reset the counter to `0`. A single stray ping no longer evicts a queued driver; it takes N consecutive out-of-fence pings.

Threshold: `queue_exit_hysteresis_threshold` (default `3`, set to `1` to disable and get legacy behaviour).

### Fix 2 — Cache-empty guard

Changed file: `drainer.rs`.

Before pushing a `PossibleExit`, the drainer now checks whether the cache has any polygons for the driver's city. If it has **zero**, we treat the situation as "unknown", not "outside", and skip pushing `PossibleExit` entirely. This prevents cache reloads / missing-city states from wiping live queues.

Toggle: `enable_queue_cache_empty_guard` (default `true`).

### Fix 3 — Atomic rank + size read (one round trip, one connection)

Changed files: `redis/commands.rs`, `domain/action/internal/location.rs`.

Added `get_driver_queue_position_and_size`, which uses a fred pipeline to issue `ZRANK` and `ZCARD` in a single round trip on the same connection. `driver_queue_position` was switched to use this function, so the returned `queue_size` and `queue_position_range` always come from the same snapshot.

### Fix 4 — Read from primary (eliminates replica-lag nulls)

Changed file: `redis/commands.rs`.

The new atomic read uses `redis.writer_pool.next()` instead of `reader_pool.next()`. Queue-position queries now go to the Redis primary, so a driver who was just `ZADD`-ed cannot appear missing due to replica lag. Queue-position is a low-QPS API relative to location-writes, so the extra primary load is negligible.

## New config knobs

All added to `AppConfig`, plumbed through `AppState` → `run_drainer`, and included in `dhall-configs/dev/location_tracking_service.dhall`:

| Knob | Default | Meaning |
|---|---|---|
| `queue_exit_hysteresis_threshold` | `3` | Number of consecutive out-of-fence pings required before a driver is actually removed from the queue. `1` disables hysteresis. |
| `enable_queue_cache_empty_guard` | `true` | When `true`, skip `PossibleExit` while the special-location cache reports zero polygons for the driver's city. |

Existing related knobs (unchanged):
- `queue_expiry_seconds` — TTL on the queue ZSET.
- `queue_position_range_offset` — half-width of the rank window returned by the API.

## How each symptom is addressed

| Symptom | Fix |
|---|---|
| Rank jumps from 17 → 35 and **stays** there under stable conditions | Fix 1 (hysteresis) — single bad ping no longer evicts. |
| Rank oscillates: 17 → 35 → 17 (single-node Redis, no replica) | Fix 1 (hysteresis) — queue-wide churn stops, rank stops swinging. |
| Rank oscillates on a deployment **with** read replicas | Fix 4 (read from primary) — reads stop bouncing across replicas with different lag. |
| Whole queue shuffles after service restart / cache reload | Fix 2 (cache-empty guard). |
| `queue_size` inconsistent with `queue_position_range` | Fix 3 (atomic pipeline). |
| API returns `null` rank for a driver who was just added | Fix 1 (churn window) + Fix 4 (replica lag if replicas are used). |

## Files touched

- `crates/location_tracking_service/src/redis/commands.rs`
  - `DriverQueueTracking` struct gained `consecutive_exit_pings`.
  - New `get_driver_queue_position_and_size` (pipelined, primary).
  - Added `RedisValue` to imports.
- `crates/location_tracking_service/src/drainer.rs`
  - `run_drainer` + `drain_driver_locations` + `drain_queue_actions` take two new parameters.
  - `PossibleExit` push is suppressed on empty-cache state.
  - `PossibleExit` handler implements hysteresis.
  - `Enter` handler resets the exit counter.
- `crates/location_tracking_service/src/domain/action/internal/location.rs`
  - `driver_queue_position` uses the atomic read.
  - `manual_queue_add` initializes the new field.
- `crates/location_tracking_service/src/environment.rs`
  - `AppConfig` and `AppState` gained the two new knobs with defaults.
- `crates/location_tracking_service/src/main.rs`
  - Plumbs knobs into `run_drainer`.
- `dhall-configs/dev/location_tracking_service.dhall`
  - Explicit values for the two new knobs.

## Verification

- `cargo check -p location_tracking_service` — clean.
- `cargo clippy -p location_tracking_service -- -D warnings` — clean.
- `cargo fmt` — applied.

## Things to watch in production after rollout

- **New log tags** to look for:
  - `[Queue Exit Hysteresis]` — driver skipped an eviction this tick, counter bumped.
  - `[Queue Exit Evict]` — threshold reached; driver actually removed.
  - `[Special Location Cache Empty Guard]` — a ping was treated as "unknown" because the cache had zero entries for that city. A burst of these during steady state indicates the cache isn't being populated for a city that should have polygons.
- **Queue size should be more stable** over time. Sudden cliffs in `ZCARD` for a queue key that used to correlate with cache reloads should no longer happen.
- **Queue-position reads** now land on the primary — monitor primary CPU/latency. If this becomes a concern, the simplest knob is to add a feature flag and switch back to `reader_pool` (accepting replica lag as a tradeoff).

## Tuning guidance

- If GPS is very clean (good antennas, dense updates): `queue_exit_hysteresis_threshold = 2` is enough.
- If GPS is noisy or polygons have tight edges: `3`–`5` is safer.
- If location-ping cadence is low (say, one every 30 s), each unit of the threshold adds 30 s of "grace" before eviction. Pick N so that `N × typical_ping_interval` is comfortably larger than the typical GPS-drift duration but small enough that a driver who genuinely leaves the airport gets removed promptly.
