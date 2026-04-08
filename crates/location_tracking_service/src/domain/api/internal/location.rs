/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::{
    delete, get, post,
    web::{Data, Json, Path},
    HttpRequest,
};

use crate::tools::error::AppError;
use crate::{
    common::types::*,
    domain::{action::internal::*, types::internal::location::*},
    environment::AppState,
};

#[get("/internal/drivers/nearby")]
async fn get_nearby_drivers(
    data: Data<AppState>,
    param_obj: Json<NearbyDriversRequest>,
    _req: HttpRequest,
) -> Result<Json<NearbyDriverResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(
        location::get_nearby_drivers(data, request_body).await?,
    ))
}

#[get("/internal/drivers/location")]
async fn get_drivers_location(
    data: Data<AppState>,
    param_obj: Json<GetDriversLocationRequest>,
    _req: HttpRequest,
) -> Result<Json<GetDriversLocationResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(
        location::get_drivers_location(data, request_body.driver_ids).await?,
    ))
}

#[post("/internal/driver/blockTill")]
async fn driver_block_till(
    data: Data<AppState>,
    param_obj: Json<DriverBlockTillRequest>,
) -> Result<Json<APISuccess>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(location::driver_block_till(data, request_body).await?))
}

#[get("/internal/trackVehicles")]
async fn track_vehicles(
    data: Data<AppState>,
    param_obj: Json<TrackVehicleRequest>,
) -> Result<Json<TrackVehiclesResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(location::track_vehicles(data, request_body).await?))
}

#[post("/internal/trackVehicles")]
async fn post_track_vehicles(
    data: Data<AppState>,
    param_obj: Json<TrackVehicleRequest>,
) -> Result<Json<TrackVehiclesResponse>, AppError> {
    let request_body = param_obj.into_inner();

    Ok(Json(location::track_vehicles(data, request_body).await?))
}

#[get("/internal/special-locations/cached")]
async fn get_cached_special_locations(
    data: Data<AppState>,
) -> Result<Json<CachedSpecialLocationsResponse>, AppError> {
    let guard = data.special_location_cache.read().await;
    let mut total_count = 0usize;
    let cities = guard
        .iter()
        .map(|(city_id, entries)| {
            total_count += entries.len();
            CachedSpecialLocationCityGroup {
                merchant_operating_city_id: city_id.0.clone(),
                count: entries.len(),
                special_locations: entries
                    .iter()
                    .map(|e| CachedSpecialLocationEntry {
                        id: e.id.0.clone(),
                        is_queue_enabled: e.is_queue_enabled,
                        is_open_market_enabled: e.is_open_market_enabled,
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(Json(CachedSpecialLocationsResponse {
        total_count,
        cities,
    }))
}

#[get("/internal/special-locations/{special_location_id}/drivers")]
async fn get_special_location_drivers(
    data: Data<AppState>,
    path: Path<(String,)>,
) -> Result<Json<SpecialLocationDriversResponse>, AppError> {
    let special_location_id = path.into_inner().0;
    Ok(Json(
        location::get_special_location_drivers(data, special_location_id).await?,
    ))
}

#[get("/internal/special-locations/{special_location_id}/queue/{vehicle_type}/drivers/{driver_id}/position")]
async fn get_driver_queue_position(
    data: Data<AppState>,
    path: Path<(String, String, String)>,
) -> Result<Json<DriverQueuePositionResponse>, AppError> {
    let (special_location_id, vehicle_type, driver_id) = path.into_inner();
    Ok(Json(
        location::driver_queue_position(data, special_location_id, vehicle_type, driver_id).await?,
    ))
}

#[get("/internal/special-locations/{special_location_id}/queue/{vehicle_type}/drivers")]
async fn get_queue_drivers(
    data: Data<AppState>,
    path: Path<(String, String)>,
) -> Result<Json<QueueDriversResponse>, AppError> {
    let (special_location_id, vehicle_type) = path.into_inner();
    Ok(Json(
        location::get_queue_drivers(data, special_location_id, vehicle_type).await?,
    ))
}

#[delete("/internal/special-locations/{special_location_id}/queue/{vehicle_type}/drivers/{merchant_id}/{driver_id}")]
async fn manual_queue_remove(
    data: Data<AppState>,
    path: Path<(String, String, String, String)>,
) -> Result<Json<APISuccess>, AppError> {
    let (special_location_id, vehicle_type, merchant_id, driver_id) = path.into_inner();
    Ok(Json(
        location::manual_queue_remove(
            data,
            special_location_id,
            vehicle_type,
            merchant_id,
            driver_id,
        )
        .await?,
    ))
}

#[post("/internal/special-locations/{special_location_id}/queue/{vehicle_type}/drivers/{merchant_id}/{driver_id}")]
async fn manual_queue_add(
    data: Data<AppState>,
    path: Path<(String, String, String, String)>,
    body: Json<ManualQueueAddRequest>,
) -> Result<Json<APISuccess>, AppError> {
    let (special_location_id, vehicle_type, merchant_id, driver_id) = path.into_inner();
    let request_body = body.into_inner();
    Ok(Json(
        location::manual_queue_add(
            data,
            special_location_id,
            vehicle_type,
            merchant_id,
            driver_id,
            request_body.queue_position,
        )
        .await?,
    ))
}
