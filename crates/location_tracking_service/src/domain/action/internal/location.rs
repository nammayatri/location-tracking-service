use std::{
    env::var,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use actix_web::web::Data;
use chrono::{DateTime, Utc};
use fred::types::{GeoPosition, GeoUnit, RedisValue, SortOrder};
use geo::{point, Intersects};

use crate::{common::{types::*, errors::AppError}, domain::types::internal::location::*};

pub async fn get_nearby_drivers(
    data: Data<AppState>,
    request_body: NearbyDriversRequest,
) -> Result<NearbyDriverResponse, AppError> {
    let location_expiry_in_seconds = var("LOCATION_EXPIRY")
        .expect("LOCATION_EXPIRY not found")
        .parse::<u64>()
        .unwrap();
    let current_bucket =
        Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / location_expiry_in_seconds;
    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: request_body.lon, y: request_body.lat));
        if intersection {
            city = multi_polygon_body.region.clone();
            break;
        }
    }

    if !intersection {
        return Err(AppError::Unserviceable);
    }
    let key = driver_loc_bucket_key(
        &request_body.merchant_id,
        &city,
        &request_body.vehicle_type.to_string(),
        &current_bucket,
    );

    let mut resp_vec: Vec<DriverLocation> = Vec::new();

    let redis_pool = data.location_redis.lock().await;
    let resp = redis_pool
        .geo_search(
            &key,
            None,
            Some(GeoPosition::from((request_body.lon, request_body.lat))),
            Some((request_body.radius, GeoUnit::Kilometers)),
            None,
            Some(SortOrder::Asc),
            None,
            true,
            true,
            false,
        )
        .await
        .unwrap();
    for item in resp {
        if let RedisValue::String(driver_id) = item.member {
            let pos = item.position.unwrap();
            let key = driver_loc_ts_key(&driver_id.to_string());
            let timestamp: String = redis_pool.get_key(&key).await.unwrap();
            let timestamp = match DateTime::parse_from_rfc3339(&timestamp) {
                Ok(x) => x.with_timezone(&Utc),
                Err(_) => Utc::now(),
            };
            let driver_location = DriverLocation {
                driver_id: driver_id.to_string(),
                lon: pos.longitude,
                lat: pos.latitude,
                coordinates_calculated_at: timestamp.clone(),
                created_at: timestamp.clone(),
                updated_at: timestamp.clone(),
                merchant_id: request_body.merchant_id.clone(),
            };
            resp_vec.push(driver_location);
        }
    }

    Ok(NearbyDriverResponse { resp : resp_vec })
}
