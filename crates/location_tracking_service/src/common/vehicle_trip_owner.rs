/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
//! Vehicle trip owner gate for bus-crew pings.
//!
//! The driver backend (dynamic-offer-driver-app) is the only writer of
//! `lts:vehicle_trip_owner:{gtfs_id}:{vehicle_no}`: trip start / rollback stores the crew's
//! person ids with `activeTrip = true`, trip end / reset stores `activeTrip = false`.
//! LTS only reads it. A ping is from the owner when the trip is active and the
//! authenticated driver id is one of `personIds`.
//!
//! LTS asks the backend about the bus (`/internal/vehicleTripOwner/resolve`) only when the
//! key is missing, or when an owner's key is older than `REVALIDATE_AFTER_SECS`. The backend
//! looks up the bus's online waybill in GIMS, writes the key itself (its crew when a trip is
//! active, `activeTrip = false` otherwise) and returns it; LTS checks the sender against it.
//! A sender who is not in `personIds` of an active key is dropped without asking.
//!
//! Calls are limited per vehicle by a lock key. The caller takes it with `SET NX EX` holding
//! `resolving` (`RESOLVE_LOCK_EXPIRY_SECS`), waits at most `RESOLVE_CALL_TIMEOUT_MS` for the
//! backend, deletes the lock on success, and on failure rewrites it as `backoff` for
//! `RESOLVE_FAILURE_BACKOFF_SECS`. Pings that find the lock taken do not call the backend:
//! with no key they are dropped while it is `resolving`, so two phones never feed the same bus,
//! and forwarded while it is `backoff`, so a backend outage degrades to today's behaviour.
//! A stale owner key keeps its owner throughout. A missing resolve URL or a Redis error
//! forwards the ping.
//!
//! `NotOwner`, `NoActiveTrip` and `Resolving` are dropped; `Owner` and `Unknown` are forwarded.
//!
//! Only bus crew (`bus_driver`, `bus_conductor`) go through this gate. Shared-cab pings skip it:
//! a shared cab has no waybill, and its run is already gated by the driver's LTS ride.

use crate::common::types::*;
use crate::environment::AppState;
use crate::outbound::external::resolve_vehicle_trip_owner;
use crate::redis::commands::{
    acquire_vehicle_trip_owner_resolve_lock, get_vehicle_trip_owner,
    get_vehicle_trip_owner_resolve_lock, release_vehicle_trip_owner_resolve_lock,
    set_vehicle_trip_owner_resolve_lock,
};
use crate::tools::error::AppError;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::time::timeout;
use tracing::warn;

const RESOLVE_LOCK_EXPIRY_SECS: i64 = 10;
const RESOLVE_FAILURE_BACKOFF_SECS: u32 = 300;
const RESOLVE_CALL_TIMEOUT_MS: u64 = 5000;
const REVALIDATE_AFTER_SECS: i64 = 1800;
const RESOLVE_LOCK_RESOLVING: &str = "resolving";
const RESOLVE_LOCK_BACKOFF: &str = "backoff";

pub fn check_owner(owner: &VehicleTripOwner, driver_id: &DriverId) -> OwnerCheck {
    if !owner.active_trip {
        OwnerCheck::NoActiveTrip
    } else if owner.person_ids.iter().any(|id| id == &driver_id.0) {
        OwnerCheck::Owner
    } else {
        OwnerCheck::NotOwner
    }
}

pub fn check_cached(
    cached: Option<&VehicleTripOwner>,
    driver_id: &DriverId,
    now: DateTime<Utc>,
) -> CachedCheck {
    let Some(owner) = cached else {
        return CachedCheck::AskBackend(BackendQuestion {
            on_failure: OwnerCheck::Unknown,
            while_resolving: OwnerCheck::Resolving,
        });
    };
    match check_owner(owner, driver_id) {
        OwnerCheck::Owner if is_stale(owner, now) => CachedCheck::AskBackend(BackendQuestion {
            on_failure: OwnerCheck::Owner,
            while_resolving: OwnerCheck::Owner,
        }),
        check => CachedCheck::Decided(check),
    }
}

fn is_stale(owner: &VehicleTripOwner, now: DateTime<Utc>) -> bool {
    owner
        .updated_at
        .is_none_or(|updated_at| (now - updated_at).num_seconds() >= REVALIDATE_AFTER_SECS)
}

pub fn check_resolved(
    question: BackendQuestion,
    outcome: &ResolveOutcome,
    driver_id: &DriverId,
) -> OwnerCheck {
    match outcome {
        ResolveOutcome::Resolved(owner) => check_owner(owner, driver_id),
        ResolveOutcome::InProgress => question.while_resolving,
        ResolveOutcome::Failed => question.on_failure,
    }
}

pub async fn owner_check(
    data: &AppState,
    driver_id: &DriverId,
    merchant_operating_city_id: &MerchantOperatingCityId,
    gtfs_id: &str,
    vehicle_no: &str,
) -> OwnerCheck {
    let cached = match get_vehicle_trip_owner(&data.redis, gtfs_id, vehicle_no).await {
        Ok(cached) => cached,
        Err(err) => {
            warn!(gtfs_id, vehicle_no, error = ?err, "vehicle trip owner read failed");
            return OwnerCheck::Unknown;
        }
    };
    let question = match check_cached(cached.as_ref(), driver_id, Utc::now()) {
        CachedCheck::Decided(check) => return check,
        CachedCheck::AskBackend(question) => question,
    };
    let outcome = resolve_owner(data, merchant_operating_city_id, gtfs_id, vehicle_no).await;
    check_resolved(question, &outcome, driver_id)
}

async fn resolve_owner(
    data: &AppState,
    merchant_operating_city_id: &MerchantOperatingCityId,
    gtfs_id: &str,
    vehicle_no: &str,
) -> ResolveOutcome {
    let Some(resolve_url) = data.vehicle_trip_owner_resolve_url.as_ref() else {
        return ResolveOutcome::Failed;
    };
    match acquire_vehicle_trip_owner_resolve_lock(
        &data.redis,
        gtfs_id,
        vehicle_no,
        RESOLVE_LOCK_RESOLVING,
        RESOLVE_LOCK_EXPIRY_SECS,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => return lock_holder_outcome(data, gtfs_id, vehicle_no).await,
        Err(err) => {
            warn!(gtfs_id, vehicle_no, error = ?err, "vehicle trip owner resolve lock failed");
            return ResolveOutcome::Failed;
        }
    }
    let request = VehicleTripOwnerReq {
        merchant_operating_city_id: merchant_operating_city_id.0.clone(),
        gtfs_id: gtfs_id.to_string(),
        vehicle_number: vehicle_no.to_string(),
    };
    let resolved = match timeout(
        Duration::from_millis(RESOLVE_CALL_TIMEOUT_MS),
        resolve_vehicle_trip_owner(resolve_url, &data.auth_api_key, request),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(AppError::RequestTimeout),
    };
    match resolved {
        Ok(owner) => {
            if let Err(err) =
                release_vehicle_trip_owner_resolve_lock(&data.redis, gtfs_id, vehicle_no).await
            {
                warn!(gtfs_id, vehicle_no, error = ?err, "vehicle trip owner resolve lock release failed");
            }
            ResolveOutcome::Resolved(owner)
        }
        Err(err) => {
            warn!(gtfs_id, vehicle_no, error = ?err, "vehicle trip owner resolve failed");
            if let Err(err) = set_vehicle_trip_owner_resolve_lock(
                &data.redis,
                gtfs_id,
                vehicle_no,
                RESOLVE_LOCK_BACKOFF,
                RESOLVE_FAILURE_BACKOFF_SECS,
            )
            .await
            {
                warn!(gtfs_id, vehicle_no, error = ?err, "vehicle trip owner resolve backoff failed");
            }
            ResolveOutcome::Failed
        }
    }
}

async fn lock_holder_outcome(data: &AppState, gtfs_id: &str, vehicle_no: &str) -> ResolveOutcome {
    match get_vehicle_trip_owner_resolve_lock(&data.redis, gtfs_id, vehicle_no).await {
        Ok(Some(value)) if value == RESOLVE_LOCK_BACKOFF => ResolveOutcome::Failed,
        Ok(_) => ResolveOutcome::InProgress,
        Err(err) => {
            warn!(gtfs_id, vehicle_no, error = ?err, "vehicle trip owner resolve lock read failed");
            ResolveOutcome::Failed
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn owner(active_trip: bool, person_ids: &[&str], updated_secs_ago: i64) -> VehicleTripOwner {
        VehicleTripOwner {
            active_trip,
            person_ids: person_ids.iter().map(|id| id.to_string()).collect(),
            updated_at: Some(Utc::now() - Duration::seconds(updated_secs_ago)),
        }
    }

    fn driver(id: &str) -> DriverId {
        DriverId(id.to_string())
    }

    #[test]
    fn crew_member_of_active_trip_is_owner() {
        let check = check_owner(
            &owner(true, &["driver", "conductor"], 0),
            &driver("conductor"),
        );
        assert_eq!(check, OwnerCheck::Owner);
    }

    #[test]
    fn ended_trip_has_no_owner_even_for_former_crew() {
        let check = check_owner(&owner(false, &["driver"], 0), &driver("driver"));
        assert_eq!(check, OwnerCheck::NoActiveTrip);
    }

    #[test]
    fn only_owner_and_unknown_are_forwarded() {
        assert!(OwnerCheck::Owner.allows_forward());
        assert!(OwnerCheck::Unknown.allows_forward());
        assert!(!OwnerCheck::Resolving.allows_forward());
        assert!(!OwnerCheck::NotOwner.allows_forward());
        assert!(!OwnerCheck::NoActiveTrip.allows_forward());
    }

    #[test]
    fn foreign_sender_is_dropped_without_asking_backend() {
        assert_eq!(
            check_cached(
                Some(&owner(true, &["driver"], 60)),
                &driver("ghost"),
                Utc::now()
            ),
            CachedCheck::Decided(OwnerCheck::NotOwner)
        );
    }

    #[test]
    fn fresh_owner_and_ended_trip_are_decided_without_backend() {
        let now = Utc::now();
        assert_eq!(
            check_cached(Some(&owner(true, &["driver"], 60)), &driver("driver"), now),
            CachedCheck::Decided(OwnerCheck::Owner)
        );
        assert_eq!(
            check_cached(Some(&owner(false, &[], 60)), &driver("driver"), now),
            CachedCheck::Decided(OwnerCheck::NoActiveTrip)
        );
    }

    #[test]
    fn missing_key_is_dropped_while_resolving_and_forwarded_on_failure() {
        let CachedCheck::AskBackend(question) = check_cached(None, &driver("driver"), Utc::now())
        else {
            panic!("expected a backend question");
        };
        assert_eq!(
            check_resolved(question, &ResolveOutcome::InProgress, &driver("driver")),
            OwnerCheck::Resolving
        );
        assert_eq!(
            check_resolved(question, &ResolveOutcome::Failed, &driver("driver")),
            OwnerCheck::Unknown
        );
    }

    #[test]
    fn stale_owner_is_revalidated_and_kept_while_resolving_or_on_failure() {
        let CachedCheck::AskBackend(question) = check_cached(
            Some(&owner(true, &["driver"], REVALIDATE_AFTER_SECS + 60)),
            &driver("driver"),
            Utc::now(),
        ) else {
            panic!("expected a revalidation");
        };
        assert_eq!(
            check_resolved(question, &ResolveOutcome::InProgress, &driver("driver")),
            OwnerCheck::Owner
        );
        assert_eq!(
            check_resolved(question, &ResolveOutcome::Failed, &driver("driver")),
            OwnerCheck::Owner
        );
    }

    #[test]
    fn backend_answer_decides_the_ping() {
        let question = BackendQuestion {
            on_failure: OwnerCheck::Unknown,
            while_resolving: OwnerCheck::Resolving,
        };
        assert_eq!(
            check_resolved(
                question,
                &ResolveOutcome::Resolved(owner(true, &["driver"], 0)),
                &driver("driver")
            ),
            OwnerCheck::Owner
        );
        assert_eq!(
            check_resolved(
                question,
                &ResolveOutcome::Resolved(owner(true, &["driver"], 0)),
                &driver("ghost")
            ),
            OwnerCheck::NotOwner
        );
        assert_eq!(
            check_resolved(
                question,
                &ResolveOutcome::Resolved(owner(false, &[], 0)),
                &driver("driver")
            ),
            OwnerCheck::NoActiveTrip
        );
    }

    #[test]
    fn backend_owner_json_deserializes_with_updated_at() {
        let json = r#"{"activeTrip":true,"waybillNo":"W1","tripNumber":2,"personIds":["p1","p2"],"updatedAt":"2026-11-05T10:00:00.123456Z"}"#;
        let parsed: VehicleTripOwner = serde_json::from_str(json).expect("valid owner json");
        assert!(parsed.active_trip);
        assert_eq!(parsed.person_ids, vec!["p1".to_string(), "p2".to_string()]);
        assert_eq!(
            parsed.updated_at.map(|at| at.timestamp()),
            Some(
                DateTime::parse_from_rfc3339("2026-11-05T10:00:00Z")
                    .expect("valid timestamp")
                    .timestamp()
            )
        );
    }

    #[test]
    fn no_active_trip_json_from_backend_deserializes() {
        let json = r#"{"activeTrip":false,"waybillNo":null,"tripNumber":null,"personIds":[],"updatedAt":"2026-11-05T10:00:00Z"}"#;
        let parsed: VehicleTripOwner = serde_json::from_str(json).expect("valid owner json");
        assert!(!parsed.active_trip);
        assert!(parsed.person_ids.is_empty());
    }
}
