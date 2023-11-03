/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use geo::{coord, Coord, LineString, MultiPolygon, Polygon};
use geojson::{Geometry, PolygonType, Position, Value};
use serde_json::from_str;
use std::fs;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Result;

use crate::common::types::MultiPolygonBody;

pub fn read_geo_polygon(config_path: &str) -> Result<Vec<MultiPolygonBody>> {
    // Read files in the directory
    let geometries = fs::read_dir(config_path).expect("Failed to read config path");

    let mut regions: Vec<MultiPolygonBody> = vec![];

    for entry in geometries {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        // Read file data into a string
        let file_path = config_path.to_owned() + "/" + &file_name;
        let mut file = File::open(file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        // Parse file data as GeoJSON

        // Filename is the region name
        let region = file_name;
        let multi_poly =
            parse_geojson_multi_polygon(&region, &contents).expect("Failed to parse GeoJSON");

        regions.push(multi_poly);
    }

    Ok(regions)
}

fn parse_geojson_multi_polygon(region: &str, geojson_str: &str) -> Result<MultiPolygonBody> {
    let geom: Geometry = from_str(geojson_str)?;

    match geom.value {
        Value::MultiPolygon(multi_polygon) => Ok(create_multipolygon_body(region, multi_polygon)),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            "GeoJSON is not a valid MultiPolygon.",
        )),
    }
}

pub fn create_multipolygon_body(region: &str, polygons: Vec<PolygonType>) -> MultiPolygonBody {
    MultiPolygonBody {
        region: region.to_string(),
        multipolygon: to_multipolygon(polygons),
    }
}

pub fn to_multipolygon(polygons: Vec<PolygonType>) -> MultiPolygon<f64> {
    MultiPolygon::new(
        polygons
            .into_iter()
            .map(to_polygon)
            .collect::<Vec<Polygon<f64>>>(),
    )
}

fn to_polygon(polygon: Vec<Vec<Position>>) -> Polygon<f64> {
    Polygon::new(
        polygon
            .into_iter()
            .map(to_line_string)
            .collect::<Vec<LineString<f64>>>()
            .into_iter()
            .flatten()
            .collect(),
        vec![],
    )
}

fn to_line_string(line_string: Vec<Position>) -> LineString<f64> {
    LineString::new(
        line_string
            .into_iter()
            .map(to_coord)
            .collect::<Vec<Coord<f64>>>(),
    )
}

fn to_coord(position: Position) -> Coord<f64> {
    coord! {x: position[0], y: position[1] }
}
