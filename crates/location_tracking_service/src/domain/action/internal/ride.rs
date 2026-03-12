/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::domain::types::ui::location::PersonType;
use crate::environment::AppState;
use crate::redis::commands::*;
use crate::tools::error::AppError;
use crate::{common::types::*, domain::types::internal::ride::*};
use actix_web::web::Data;
use chrono::Utc;
use std::str::FromStr;

pub async fn ride_create(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideCreateRequest,
) -> Result<APISuccess, AppError> {
    if let Some(false) | None = request_body.is_future_ride {
        set_ride_details_for_driver(
            &data.redis,
            &data.redis_expiry,
            &request_body.merchant_id,
            &request_body.driver_id,
            ride_id.to_owned(),
            RideStatus::NEW,
            request_body.ride_info,
        )
        .await?;
    }

    let driver_details = DriverDetails {
        driver_id: request_body.driver_id,
    };

    set_on_ride_driver_details(&data.redis, &data.redis_expiry, &ride_id, driver_details).await?;

    Ok(APISuccess::default())
}

pub async fn ride_start(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> Result<APISuccess, AppError> {
    set_ride_details_for_driver(
        &data.redis,
        &data.redis_expiry,
        &request_body.merchant_id,
        &request_body.driver_id,
        ride_id,
        RideStatus::INPROGRESS,
        request_body.ride_info,
    )
    .await?;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    ride_id: RideId,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    let mut on_ride_driver_locations = get_on_ride_driver_locations(
        &data.redis,
        &request_body.driver_id,
        &request_body.merchant_id,
        data.batch_size,
    )
    .await?;

    on_ride_driver_locations.push(Point {
        lat: request_body.lat,
        lon: request_body.lon,
    });

    ride_cleanup(
        &data.redis,
        &request_body.merchant_id,
        &request_body.driver_id,
        &ride_id,
        &request_body.ride_info,
    )
    .await?;

    if let Some(next_ride_id) = request_body.next_ride_id {
        let ride_details_request = RideDetailsRequest {
            ride_id: next_ride_id,
            ride_status: RideStatus::NEW,
            is_future_ride: Some(false),
            merchant_id: request_body.merchant_id,
            driver_id: request_body.driver_id.clone(),
            lat: request_body.lat,
            lon: request_body.lon,
            ride_info: None,
        };
        ride_details(data, ride_details_request).await?;
    }

    Ok(RideEndResponse {
        ride_id,
        driver_id: request_body.driver_id,
        loc: on_ride_driver_locations,
    })
}

pub async fn get_driver_locations(
    _ride_id: RideId,
    data: Data<AppState>,
    request_body: DriverLocationRequest,
) -> Result<DriverLocationResponse, AppError> {
    let on_ride_driver_locations = get_on_ride_driver_locations(
        &data.redis,
        &request_body.driver_id,
        &request_body.merchant_id,
        data.batch_size,
    )
    .await?;

    let driver_location_details = get_driver_location(&data.redis, &request_body.driver_id).await?;

    Ok(DriverLocationResponse {
        loc: on_ride_driver_locations,
        timestamp: driver_location_details.map(|driver_location_details| {
            driver_location_details.driver_last_known_location.timestamp
        }),
    })
}

pub async fn ride_details(
    data: Data<AppState>,
    request_body: RideDetailsRequest,
) -> Result<APISuccess, AppError> {
    let driver_id = request_body.driver_id.clone();

    if let RideStatus::CANCELLED = request_body.ride_status {
        ride_cleanup(
            &data.redis,
            &request_body.merchant_id,
            &driver_id,
            &request_body.ride_id,
            &request_body.ride_info,
        )
        .await?;

        if let Some(driver_location) = get_driver_location(&data.redis, &driver_id).await? {
            set_driver_last_location_update(
                &data.redis,
                &data.redis_expiry,
                &driver_id,
                &request_body.merchant_id,
                &driver_location.driver_last_known_location.location,
                &driver_location.driver_last_known_location.timestamp,
                &driver_location.blocked_till,
                driver_location.stop_detection,
                &driver_location.ride_status,
                &None,
                &driver_location.detection_state,
                &driver_location.anti_detection_state,
                &driver_location.violation_trigger_flag,
                &driver_location.driver_pickup_distance,
                &driver_location.driver_last_known_location.bear,
                &driver_location.driver_last_known_location.vehicle_type,
                &driver_location.driver_last_known_location.group_id,
                &driver_location.driver_last_known_location.group_id2,
            )
            .await?;
        }
    } else {
        let driver_details = DriverDetails {
            driver_id: driver_id.clone(),
        };

        set_on_ride_driver_details(
            &data.redis,
            &data.redis_expiry,
            &request_body.ride_id,
            driver_details,
        )
        .await?;

        if let Some(false) | None = request_body.is_future_ride {
            set_ride_details_for_driver(
                &data.redis,
                &data.redis_expiry,
                &request_body.merchant_id,
                &driver_id,
                request_body.ride_id.to_owned(),
                request_body.ride_status,
                request_body.ride_info,
            )
            .await?;

            if let Some(driver_location) = get_driver_location(&data.redis, &driver_id).await? {
                set_driver_last_location_update(
                    &data.redis,
                    &data.redis_expiry,
                    &driver_id,
                    &request_body.merchant_id,
                    &driver_location.driver_last_known_location.location,
                    &driver_location.driver_last_known_location.timestamp,
                    &driver_location.blocked_till,
                    driver_location.stop_detection,
                    &driver_location.ride_status,
                    &Some(RideNotificationStatus::Idle),
                    &None,
                    &None,
                    &None,
                    &None,
                    &driver_location.driver_last_known_location.bear,
                    &driver_location.driver_last_known_location.vehicle_type,
                    &driver_location.driver_last_known_location.group_id,
                    &driver_location.driver_last_known_location.group_id2,
                )
                .await?;
            }
        }
    }

    Ok(APISuccess::default())
}

/// Store SOS broadcaster config in Redis. Called internally from `entity_upsert` when
/// `EntityStart` carries a `broadcaster_config` for an SOS entity.
async fn store_broadcaster_config(
    redis: &shared::redis::types::RedisConnectionPool,
    entity_id: &str,
    req: BroadcasterConfigRequest,
    default_expiry: u32,
) -> Result<(), AppError> {
    let now_secs = Utc::now().timestamp();
    let expiry_secs = req
        .expires_at
        .map(|exp| (exp - now_secs).max(60) as u32)
        .unwrap_or(default_expiry);

    if req.expires_at.map(|exp| exp <= now_secs).unwrap_or(false) {
        tracing::warn!(
            entity_id,
            "Broadcaster config stored with expires_at already in the past; using 60s TTL"
        );
    }

    let config = SosBroadcasterConfig {
        external_reference_id: req.external_reference_id,
        base_url: req.base_url,
        access_token: req.access_token,
        token_expires_at: req.token_expires_at,
        ny_reauth_url: req.ny_reauth_url,
        ny_api_key: req.ny_api_key,
        merchant_operating_city_id: req.merchant_operating_city_id,
        polling_interval_secs: req.polling_interval_secs,
        time_diff_secs: req.time_diff_secs,
        last_trace_ts: None,
        expires_at: req.expires_at,
        provider: req.provider,
    };

    crate::redis::commands::set_sos_broadcaster_config(redis, entity_id, &config, expiry_secs).await
}

/// Generic entity upsert: create (ride only), start, or end. Uses generic Redis keys; rider-only for now.
pub async fn entity_upsert(
    person_type: &str,
    entity_type: &str,
    entity_id_str: &str,
    data: Data<AppState>,
    request_body: EntityUpsertRequest,
) -> Result<EntityUpsertResponse, AppError> {
    let entity_id = match entity_type.to_lowercase().as_str() {
        "ride" => EntityId::Ride(RideId(entity_id_str.to_string())),
        "sos" => EntityId::Sos(SosId(entity_id_str.to_string())),
        _ => {
            return Err(AppError::InvalidRequest(format!(
                "Unknown entity_type: {}",
                entity_type
            )))
        }
    };
    let person_type = PersonType::from_str(person_type)
        .map_err(|_| AppError::InvalidRequest(format!("Invalid person_type: {}", person_type)))?;
    let person_id = PersonId(request_body.person_id.clone());
    let merchant_id = &request_body.merchant_id;

    match &request_body.entity_info {
        EntityInfo::EntityCreate => {
            if entity_type.eq_ignore_ascii_case("sos") {
                return Err(AppError::InvalidRequest(
                    "EntityCreate is not supported for SOS".to_string(),
                ));
            }
            let entry = EntityEntry {
                entity_id: entity_id.clone(),
                broadcaster_ids: Vec::new(),
            };
            add_entity_for_person(
                &data.redis,
                merchant_id,
                person_type,
                &person_id,
                entity_type,
                entry,
                &data.redis_expiry,
            )
            .await?;
            set_person_by_entity(
                &data.redis,
                entity_type,
                entity_id_str,
                &person_id,
                &data.redis_expiry,
            )
            .await?;
            Ok(EntityUpsertResponse::APISuccess(APISuccess::default()))
        }
        EntityInfo::EntityStart => {
            let broadcaster_ids = if entity_type.eq_ignore_ascii_case("sos") {
                vec![entity_id_str.to_string()]
            } else {
                Vec::new()
            };
            let entry = EntityEntry {
                entity_id: entity_id.clone(),
                broadcaster_ids,
            };
            add_entity_for_person(
                &data.redis,
                merchant_id,
                person_type,
                &person_id,
                entity_type,
                entry,
                &data.redis_expiry,
            )
            .await?;
            set_person_by_entity(
                &data.redis,
                entity_type,
                entity_id_str,
                &person_id,
                &data.redis_expiry,
            )
            .await?;
            if let Some(cfg) = request_body.broadcaster_config {
                store_broadcaster_config(&data.redis, entity_id_str, cfg, data.redis_expiry)
                    .await?;
            }
            Ok(EntityUpsertResponse::APISuccess(APISuccess::default()))
        }
        EntityInfo::EntityEnd { lat, lon } => {
            let batch_size = data.batch_size;
            let mut loc = get_entity_locations(
                &data.redis,
                merchant_id,
                person_type,
                &person_id,
                batch_size,
            )
            .await?;
            loc.push(Point {
                lat: *lat,
                lon: *lon,
            });
            // Remove only this entity type from the map, then clean up related keys.
            remove_entity_for_person(
                &data.redis,
                merchant_id,
                person_type,
                &person_id,
                entity_type,
            )
            .await?;
            entity_cleanup_generic(
                &data.redis,
                merchant_id,
                person_type,
                &person_id,
                entity_type,
                entity_id_str,
            )
            .await?;
            Ok(EntityUpsertResponse::EntityEnd { loc })
        }
    }
}
