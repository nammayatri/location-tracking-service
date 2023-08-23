use super::types::*;
use geo::{point, Intersects};
use shared::tools::error::AppError;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn get_city(
    lat: Latitude,
    lon: Longitude,
    polygon: Vec<MultiPolygonBody>,
) -> Result<String, AppError> {
    let mut city = String::new();
    let mut intersection = false;

    for multi_polygon_body in polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: lon, y: lat));
        if intersection {
            city = multi_polygon_body.region.clone();
            break;
        }
    }

    if !intersection {
        return Err(AppError::Unserviceable);
    }

    Ok(city)
}

pub fn get_current_bucket(location_expiry_in_seconds: u64) -> Result<u64, AppError> {
    Ok(Duration::as_secs(
        &SystemTime::elapsed(&UNIX_EPOCH)
            .map_err(|err| AppError::InternalError(err.to_string()))?,
    ) / location_expiry_in_seconds)
}
