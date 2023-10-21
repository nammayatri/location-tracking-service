/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use chrono::Utc;
use geo::{point, Intersects};
use shared::tools::error::AppError;
use std::f64::consts::PI;

pub fn get_city(
    lat: &Latitude,
    lon: &Longitude,
    polygon: &Vec<MultiPolygonBody>,
) -> Result<CityName, AppError> {
    let mut city = String::new();
    let mut intersection = false;

    let Latitude(lat) = *lat;
    let Longitude(lon) = *lon;

    for multi_polygon_body in polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: lon, y: lat));
        if intersection {
            city = multi_polygon_body.region.to_string();
            break;
        }
    }

    if !intersection {
        return Err(AppError::Unserviceable(lat, lon));
    }

    Ok(CityName(city))
}

pub fn is_blacklist_for_special_zone(
    merchant_id: &MerchantId,
    blacklist_merchants: &[MerchantId],
    lat: &Latitude,
    lon: &Longitude,
    polygon: &Vec<MultiPolygonBody>,
) -> bool {
    let blacklist_merchant = blacklist_merchants.contains(merchant_id);

    if blacklist_merchant {
        let mut intersection = false;

        let Latitude(lat) = *lat;
        let Longitude(lon) = *lon;

        for multi_polygon_body in polygon {
            intersection = multi_polygon_body
                .multipolygon
                .intersects(&point!(x: lon, y: lat));
            if intersection {
                break;
            }
        }

        if !intersection {
            return false;
        }

        true
    } else {
        false
    }
}

pub fn get_current_bucket(location_expiry_in_seconds: &u64) -> u64 {
    Utc::now().timestamp() as u64 / location_expiry_in_seconds
}

pub fn get_bucket_from_timestamp(
    location_expiry_in_seconds: &u64,
    TimeStamp(ts): TimeStamp,
) -> u64 {
    ts.timestamp() as u64 / location_expiry_in_seconds
}

fn deg2rad(degrees: f64) -> f64 {
    degrees * PI / 180.0
}

pub fn distance_between_in_meters(latlong1: &Point, latlong2: &Point) -> f64 {
    // Calculating using haversine formula
    // Radius of Earth in meters
    let r: f64 = 6371000.0;

    let Latitude(lat1) = latlong1.lat;
    let Longitude(lon1) = latlong1.lon;
    let Latitude(lat2) = latlong2.lat;
    let Longitude(lon2) = latlong2.lon;

    let dlat = deg2rad(lat2 - lat1);
    let dlon = deg2rad(lon2 - lon1);

    let rlat1 = deg2rad(lat1);
    let rlat2 = deg2rad(lat2);

    let sq = |x: f64| x * x;

    // Calculated distance is real (not imaginary) when 0 <= h <= 1
    // Ideally in our use case h wouldn't go out of bounds
    let h = sq((dlat / 2.0).sin()) + rlat1.cos() * rlat2.cos() * sq((dlon / 2.0).sin());

    2.0 * r * h.sqrt().atan2((1.0 - h).sqrt())
}
