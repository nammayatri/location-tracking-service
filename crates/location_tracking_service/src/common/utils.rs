/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
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
