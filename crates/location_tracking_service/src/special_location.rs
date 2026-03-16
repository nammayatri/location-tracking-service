/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::outbound::types::{SpecialLocationFull, SpecialLocationId};
use geo::Intersects;
use geojson::GeoJson;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// One parsed special location (geometry + metadata) for in-memory lookup.
#[derive(Clone)]
pub struct SpecialLocationEntry {
    pub id: SpecialLocationId,
    pub is_open_market_enabled: bool,
    pub is_queue_enabled: bool,
    pub multipolygon: geo::MultiPolygon<f64>,
}

/// Cache: per merchant_operating_city_id, list of special locations (with geometry).
pub type SpecialLocationCache =
    Arc<RwLock<FxHashMap<MerchantOperatingCityId, Vec<SpecialLocationEntry>>>>;

/// Build cache from API response. Only keeps items with both merchant_operating_city_id and geo_json.
pub fn build_special_location_cache(
    list: Vec<SpecialLocationFull>,
) -> FxHashMap<MerchantOperatingCityId, Vec<SpecialLocationEntry>> {
    let mut by_city: FxHashMap<MerchantOperatingCityId, Vec<SpecialLocationEntry>> =
        FxHashMap::default();
    for loc in list {
        let Some(ref mocid_str) = loc.merchant_operating_city_id else { continue };
        let Some(ref geo_json_str) = loc.geo_json else { continue };
        let multipolygon = match parse_geojson_to_multipolygon(geo_json_str) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let entry = SpecialLocationEntry {
            id: loc.id.clone(),
            is_open_market_enabled: loc.is_open_market_enabled,
            is_queue_enabled: loc.is_queue_enabled,
            multipolygon,
        };
        by_city
            .entry(MerchantOperatingCityId(mocid_str.clone()))
            .or_default()
            .push(entry);
    }
    by_city
}

fn parse_geojson_to_multipolygon(s: &str) -> Result<geo::MultiPolygon<f64>, ()> {
    let geojson: GeoJson = serde_json::from_str(s).map_err(|_| ())?;
    match geojson {
        GeoJson::Geometry(geom) => match geom.value {
            geojson::Value::MultiPolygon(coords) => {
                let polygons = coords
                    .into_iter()
                    .map(|ring| {
                        let exterior = ring
                            .first()
                            .map(|ex| {
                                geo::LineString(
                                    ex.iter().map(|p| geo::Coord { x: p[0], y: p[1] }).collect(),
                                )
                            })
                            .unwrap_or_else(|| geo::LineString(vec![]));
                        let interiors = ring.get(1..).unwrap_or(&[]).to_vec();
                        let interiors: Vec<geo::LineString<f64>> = interiors
                            .iter()
                            .map(|in_ring| {
                                geo::LineString(
                                    in_ring
                                        .iter()
                                        .map(|p| geo::Coord { x: p[0], y: p[1] })
                                        .collect(),
                                )
                            })
                            .collect();
                        geo::Polygon::new(exterior, interiors)
                    })
                    .collect();
                Ok(geo::MultiPolygon::new(polygons))
            }
            geojson::Value::Polygon(rings) => {
                let exterior = rings.first().map(|ex| {
                    geo::LineString(ex.iter().map(|p| geo::Coord { x: p[0], y: p[1] }).collect())
                });
                let interiors = rings.get(1..).unwrap_or(&[]).to_vec();
                let interiors: Vec<geo::LineString<f64>> = interiors
                    .iter()
                    .map(|r| {
                        geo::LineString(r.iter().map(|p| geo::Coord { x: p[0], y: p[1] }).collect())
                    })
                    .collect();
                let poly = exterior.map(|ex| geo::Polygon::new(ex, interiors));
                Ok(geo::MultiPolygon::new(poly.into_iter().collect()))
            }
            _ => Err(()),
        },
        _ => Err(()),
    }
}

/// Returns the first matching special location (if any) that contains the point.
pub fn lookup_special_location<'a>(
    cache: &'a FxHashMap<MerchantOperatingCityId, Vec<SpecialLocationEntry>>,
    merchant_operating_city_id: &MerchantOperatingCityId,
    lat: &Latitude,
    lon: &Longitude,
) -> Option<&'a SpecialLocationEntry> {
    let entries = cache.get(merchant_operating_city_id)?;
    let Latitude(lat_f) = *lat;
    let Longitude(lon_f) = *lon;
    let point = geo::point!(x: lon_f, y: lat_f);
    entries
        .iter()
        .find(|entry| entry.multipolygon.intersects(&point))
}
