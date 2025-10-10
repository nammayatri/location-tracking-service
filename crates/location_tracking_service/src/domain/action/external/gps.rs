/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::domain::action::ui::location::update_driver_location;
use crate::domain::types::ui::location::UpdateDriverLocationRequest;
use crate::environment::AppState;
use crate::redis::keys::driver_info_by_plate_key;
use crate::tools::error::AppError;
use crate::tools::prometheus::GPS_UPDATES_IGNORED_NO_ACTIVE_RIDE;
use actix_web::{web::Data, HttpResponse};
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::future::try_join_all;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ExternalGPSLocationReq {
    pub imei: String,
    pub dt_server: String, // "2025-09-16 12:51:23"
    pub lat: f64,
    pub lng: f64,
    pub speed: Option<i32>,
    pub plate_number: String,
    pub altitude: Option<String>,
    pub angle: Option<f64>,
    pub name: Option<String>,
    pub active: Option<String>,
    pub address: Option<String>,
}

pub async fn handle_external_gps_location(
    api_key: Option<String>,
    gps_batch: Vec<ExternalGPSLocationReq>,
    app_state: Data<AppState>,
) -> Result<HttpResponse, AppError> {
    // Validate API key
    let provided_api_key = api_key.ok_or(AppError::MissingApiKey)?;
    if provided_api_key != app_state.external_gps_api_key {
        return Err(AppError::InvalidApiKey);
    }

    info!(
        tag = "[GPS Batch Request]",
        "Received {} GPS location updates",
        gps_batch.len()
    );

    // Group by plate_number for efficient processing
    let mut plate_groups: FxHashMap<String, Vec<ExternalGPSLocationReq>> = FxHashMap::default();
    for gps_data in gps_batch {
        plate_groups
            .entry(gps_data.plate_number.clone())
            .or_default()
            .push(gps_data);
    }

    // Step 1: Get all unique plate numbers
    let plate_numbers: Vec<String> = plate_groups.keys().cloned().collect();

    // Step 2: Batch fetch all driver info using MGET (single network call)
    let cache_keys: Vec<String> = plate_numbers
        .iter()
        .map(|plate| driver_info_by_plate_key(plate))
        .collect();

    let cached_results = app_state
        .redis
        .mget_keys::<DriverByPlateResp>(cache_keys)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

    // Separate cache hits from misses
    let mut drivers_info = Vec::new();
    let mut uncached_plates = Vec::new();

    for (plate_number, cached_driver_info) in plate_numbers.iter().zip(cached_results.into_iter()) {
        match cached_driver_info {
            Some(driver_info) => {
                info!(
                    tag = "[Cache Hit]",
                    "Found cached driver info for plate {}", plate_number
                );
                drivers_info.push(driver_info);
            }
            None => {
                uncached_plates.push(plate_number.clone());
            }
        }
    }

    // Skip GPS updates for plates without active rides
    // Cache is populated at ride start, so missing cache = no active ride
    if !uncached_plates.is_empty() {
        GPS_UPDATES_IGNORED_NO_ACTIVE_RIDE.inc_by(uncached_plates.len() as u64);
        warn!(
            tag = "[GPS Ignored - No Active Ride]",
            "Skipping GPS updates for {} plates without active rides: {:?}",
            uncached_plates.len(),
            uncached_plates
        );
    }

    // Step 3: Process each vehicle's locations with its driver info
    let tasks: Vec<_> = drivers_info
        .into_iter()
        .filter_map(|driver_info| {
            plate_groups
                .remove(&driver_info.bus_number.clone().unwrap_or_default())
                .map(|locations| {
                    process_vehicle_locations(driver_info, locations, app_state.clone())
                })
        })
        .collect();

    // Execute all location processing tasks in parallel
    try_join_all(tasks).await?;

    info!(
        tag = "[GPS Batch Processed]",
        "Successfully processed all GPS updates"
    );
    Ok(HttpResponse::Ok().body("SUCCESS"))
}

async fn process_vehicle_locations(
    driver_info: DriverByPlateResp,
    locations: Vec<ExternalGPSLocationReq>,
    app_state: Data<AppState>,
) -> Result<(), AppError> {
    let plate_number = driver_info.bus_number.clone().unwrap_or_default();

    info!(
        tag = "[Processing Vehicle]",
        "Processing {} GPS locations for vehicle {}",
        locations.len(),
        plate_number
    );

    if locations.is_empty() {
        warn!("No GPS location updates for vehicle {}", plate_number);
        return Ok(());
    }

    // Step 1: Convert GPS data to LocationTrackingService format
    let location_updates = convert_gps_to_location_updates(locations)?;

    // Step 2: Call simplified location update pipeline (no token needed)
    update_driver_location(
        DriverId(driver_info.driver_id),
        MerchantId(driver_info.merchant_id),
        driver_info.vehicle_service_tier_type,
        app_state,
        location_updates,
        DriverMode::ONLINE,
        driver_info.group_id,
    )
    .await?;

    info!(
        tag = "[GPS Processed]",
        "Successfully processed GPS data for vehicle {}", plate_number
    );

    Ok(())
}

fn convert_gps_to_location_updates(
    gps_locations: Vec<ExternalGPSLocationReq>,
) -> Result<Vec<UpdateDriverLocationRequest>, AppError> {
    let mut location_updates = Vec::new();

    for gps in gps_locations {
        // Validate coordinate ranges
        if !(-90.0..=90.0).contains(&gps.lat) || !(-180.0..=180.0).contains(&gps.lng) {
            return Err(AppError::InvalidGPSData(format!(
                "Coordinates out of range: lat={}, lng={}",
                gps.lat, gps.lng
            )));
        }

        // Parse timestamp
        let timestamp = parse_gps_timestamp(&gps.dt_server)?;

        // Convert speed from km/h to m/s
        let speed = gps.speed.map(|s| SpeedInMeterPerSecond(s as f64 / 3.6));

        // Bearing is already parsed as f64
        let bearing = gps.angle.map(Direction);

        location_updates.push(UpdateDriverLocationRequest {
            pt: Point {
                lat: Latitude(gps.lat),
                lon: Longitude(gps.lng),
            },
            ts: TimeStamp(timestamp),
            v: speed,
            acc: None,
            bear: bearing,
        });
    }

    Ok(location_updates)
}

fn parse_gps_timestamp(dt_server: &str) -> Result<DateTime<Utc>, AppError> {
    // Parse "2025-09-16 12:51:23" format
    let naive_dt = NaiveDateTime::parse_from_str(dt_server, "%Y-%m-%d %H:%M:%S").map_err(|_| {
        AppError::InvalidGPSData(format!("Invalid timestamp format: {}", dt_server))
    })?;

    Ok(naive_dt.and_utc())
}
