use crate::environment::BuisnessConfigs;

/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use crate::tools::error::AppError;
use cac_client as cac;
use geo::{point, Intersects};
use rand::Rng;
use serde_json::{json, Map, Value};
use shared::tools::error::AppError;
use std::{f64::consts::PI, sync::Arc};
use superposition_client as spclient;

/// Retrieves the name of the city based on latitude and longitude coordinates.
///
/// This function goes through each multi-polygon body in the provided vector,
/// checking if the given latitude and longitude intersect with any of them.
/// If an intersection is found, it retrieves the name of the region (city) associated with
/// that multi-polygon body.
///
/// # Arguments
///
/// * `lat` - Latitude coordinate.
/// * `lon` - Longitude coordinate.
/// * `polygon` - A vector of multi-polygon bodies, each associated with a city or region.
///
/// # Returns
///
/// * `Ok(CityName)` - If an intersection is found, returns the name of the city or region
/// inside a `CityName` wrapper as a result.
/// * `Err(AppError)` - If no intersection is found, returns an error with the type `AppError::Unserviceable`,
/// including the latitude and longitude that were checked.
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

    if intersection {
        Ok(CityName(city))
    } else {
        Err(AppError::Unserviceable(lat, lon))
    }
}

/// Checks if a merchant is blacklisted for a special zone based on their ID and location,
/// if blacklisted then their location will not be drained and will not be part of nearby driver pooling.
///
/// This function will first check if the merchant is in the blacklist, and if they are,
/// it will further check if the merchant's location intersects with any of the given multi-polygon bodies.
///
/// # Arguments
///
/// * `merchant_id` - The ID of the merchant to check.
/// * `blacklist_merchants` - A slice of blacklisted merchant IDs.
/// * `lat` - Latitude of the merchant's location.
/// * `lon` - Longitude of the merchant's location.
/// * `polygon` - A vector of multi-polygon bodies representing restricted areas.
///
/// # Returns
///
/// Returns `true` if the merchant is blacklisted and their location intersects
/// with any of the provided multi-polygon bodies. Otherwise, returns `false`.
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

        intersection
    } else {
        false
    }
}

/// Computes a bucket identifier for a given timestamp based on a specified expiry duration.
///
/// The function divides the given timestamp by the `location_expiry_in_seconds` to determine
/// which "bucket" or interval the timestamp falls into. This is useful for partitioning or
/// grouping events that occur within certain time intervals.
///
/// # Arguments
///
/// * `location_expiry_in_seconds` - The duration, in seconds, that determines the length of each bucket.
/// * `TimeStamp(ts)` - A timestamp wrapped in the `TimeStamp` type.
///
/// # Returns
///
/// The bucket identifier as a `u64` value, representing which interval the timestamp belongs to.
///
/// # Examples
///
/// ```
/// let expiry_duration = 3600; // 1 hour
/// let sample_timestamp = TimeStamp(chrono::Utc::now());
///
/// let bucket = get_bucket_from_timestamp(&expiry_duration, sample_timestamp);
/// println!("Timestamp belongs to bucket: {}", bucket);
/// ```
///
/// # Notes
///
/// The function assumes that the timestamp is represented in seconds since the Unix epoch.
pub fn get_bucket_from_timestamp(
    location_expiry_in_seconds: &u64,
    TimeStamp(ts): TimeStamp,
) -> u64 {
    ts.timestamp() as u64 / location_expiry_in_seconds
}

/// Calculates the distance between two geographical points in meters.
///
/// The function utilizes the haversine formula to compute the great-circle
/// distance between two points on the surface of a sphere, which in this case
/// is the Earth. This method provides a reliable calculation for short distances.
///
/// # Arguments
///
/// * `latlong1` - The first geographical point, represented as a `Point` with latitude and longitude.
/// * `latlong2` - The second geographical point, similarly represented.
///
/// # Returns
///
/// The calculated distance between the two points in meters.
///
/// # Examples
///
/// ```
/// let point1 = Point { lat: Latitude(34.0522), lon: Longitude(-118.2437) }; // Los Angeles
/// let point2 = Point { lat: Latitude(40.7128), lon: Longitude(-74.0060) };  // New York
///
/// let distance = distance_between_in_meters(&point1, &point2);
/// println!("Distance: {} meters", distance);
/// ```
///
/// # Notes
///
/// The function assumes the Earth as a perfect sphere with a radius of 6,371,000 meters.
/// For very precise measurements, other methods or refinements may be necessary.
pub fn distance_between_in_meters(latlong1: &Point, latlong2: &Point) -> f64 {
    // Calculating using haversine formula
    // Radius of Earth in meters
    let r: f64 = 6371000.0;

    let Latitude(lat1) = latlong1.lat;
    let Longitude(lon1) = latlong1.lon;
    let Latitude(lat2) = latlong2.lat;
    let Longitude(lon2) = latlong2.lon;

    let deg2rad = |degrees: f64| -> f64 { degrees * PI / 180.0 };

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

/// Takes a vector of `Option<T>` and returns a vector of unwrapped `T` values, filtering out `None`.
///
/// # Examples
///
/// ```
/// let options = vec![Some(1), None, Some(2), Some(3), None];
/// let unwrapped = cat_maybes(options);
/// assert_eq!(unwrapped, vec![1, 2, 3]);
/// ```
///
/// # Type Parameters
///
/// - `T`: The type of the values contained in the `Option`.
///
/// # Arguments
///
/// - `options`: A vector of `Option<T>` that may contain `Some(T)` or `None` values.
///
/// # Returns
///
/// A vector of unwrapped `T` values, with `None` values omitted.
///
pub fn cat_maybes<T>(options: Vec<Option<T>>) -> Vec<T> {
    options.into_iter().flatten().collect()
}

pub fn get_cac_client(tenant_name: String) -> Result<Arc<cac::Client>, String> {
    cac::CLIENT_FACTORY.get_client(tenant_name)
}

pub async fn get_superposition_client(
    tenant_name: String,
) -> Result<Arc<spclient::Client>, String> {
    spclient::CLIENT_FACTORY.get_client(tenant_name).await
}

pub async fn get_config_from_cac_client(
    tenant_name: String,
    key: String,
    mut ctx: Map<String, Value>,
    buisness_cfgs: BuisnessConfigs,
    toss: i8,
) -> Result<Value, AppError> {
    let cacclient: Result<Arc<cac::Client>, String> = get_cac_client(tenant_name.clone());
    let superclient: Result<Arc<spclient::Client>, String> =
        get_superposition_client(tenant_name.clone()).await;
    match (cacclient, superclient) {
        (Ok(cacclient), Ok(superclient)) => {
            let variant_ids = superclient
                .get_applicable_variant(&json!(ctx.clone()), toss)
                .await;
            ctx.insert(String::from("variantIds"), variant_ids.clone().into());
            let res = cacclient.eval(ctx.clone());
            match res {
                Ok(res) => match res.get(&key) {
                    Some(val) => Ok(val.clone()),
                    _ => {
                        log::error!("Key does not exist in cac client's response for tenant {}, trying fetch the same key from default config", tenant_name);
                        get_default_config(tenant_name, key, buisness_cfgs).await
                    }
                },
                _ => {
                    log::error!("Failed to fetch config from cac client for tenant {}, trying fetch default config", tenant_name);
                    get_default_config(tenant_name, key, buisness_cfgs).await
                }
            }
        }
        (Err(_), Ok(_)) => {
            log::error!(
                "Failed to fetch instance of cac client for tenant {}",
                tenant_name
            );
            get_default_config(tenant_name, key, buisness_cfgs).await
        }
        (Ok(_), Err(_)) => {
            log::error!(
                "Failed to fetch instance of superposition client for tenant {}",
                tenant_name
            );
            get_default_config(tenant_name, key, buisness_cfgs).await
        }
        _ => {
            log::error!(
                "Failed to fetch instance of cac client and superposition client for tenant {}",
                tenant_name
            );
            get_default_config(tenant_name, key, buisness_cfgs).await
        }
    }
}

pub async fn get_default_config(
    tenant_name: String,
    key: String,
    def_buisness_cfgs: BuisnessConfigs,
) -> Result<Value, AppError> {
    println!("Fetching default config for tenant {}", tenant_name);
    def_buisness_cfgs.get_field(&key)
}

pub fn get_random_number() -> i8 {
    let mut rng = rand::thread_rng();
    rng.gen_range(1..100)
}
