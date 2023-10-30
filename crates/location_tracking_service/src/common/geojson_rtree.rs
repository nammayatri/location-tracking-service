/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use geo::{
    bounding_rect::BoundingRect, euclidean_distance::EuclideanDistance, Coord, LineString, Polygon,
};
use geojson::{feature::Id, Bbox, JsonObject, LineStringType, PointType, PolygonType};
use num_traits::Float;
use rstar::{Envelope, Point, PointDistance, RTreeObject, AABB};
use std::convert::TryFrom;

#[derive(Clone, Debug, PartialEq)]
pub struct PolygonFeature {
    bbox: Bbox,
    polygon: PolygonType,
    pub id: Option<Id>,
    pub properties: Option<JsonObject>,
    pub foreign_members: Option<JsonObject>,
}

impl PolygonFeature {
    pub fn polygon(&self) -> &PolygonType {
        &self.polygon
    }

    pub fn geo_polygon(&self) -> Polygon<f64> {
        create_geo_polygon(&self.polygon)
    }
}

impl From<PolygonFeature> for geojson::Feature {
    fn from(val: PolygonFeature) -> Self {
        let geometry = geojson::Geometry::new(geojson::Value::Polygon(val.polygon));

        geojson::Feature {
            id: val.id,
            properties: val.properties,
            foreign_members: val.foreign_members,
            geometry: Some(geometry),
            bbox: Some(val.bbox),
        }
    }
}

impl TryFrom<geojson::Feature> for PolygonFeature {
    type Error = GeoJsonConversionError;

    fn try_from(feature: geojson::Feature) -> Result<Self, Self::Error> {
        <Self as GenericFeature<PolygonFeature, PolygonType>>::try_from(feature)
    }
}

impl GenericFeature<PolygonFeature, PolygonType> for PolygonFeature {
    fn take_geometry_type(
        feature: &mut geojson::Feature,
    ) -> Result<PolygonType, GeoJsonConversionError> {
        if let geojson::Value::Polygon(polygon) = feature
            .geometry
            .take()
            .ok_or_else(|| {
                let id = feature.id.clone();
                GeoJsonConversionError::MissingGeometry(id)
            })?
            .value
        {
            Ok(polygon)
        } else {
            Err(GeoJsonConversionError::IncorrectGeometryValue(
                "Error: did not find Polygon feature".into(),
            ))
        }
    }

    fn check_geometry(
        geometry: &PolygonType,
        feature: &geojson::Feature,
    ) -> Result<(), GeoJsonConversionError> {
        if geometry
            .iter()
            .any(|line| line.iter().any(|p| p.len() != 2))
        {
            let id = feature.id.clone();
            return Err(GeoJsonConversionError::MalformedGeometry(id));
        }

        if geometry.is_empty() || geometry.iter().any(|v| v.is_empty()) {
            let id = feature.id.clone();
            return Err(GeoJsonConversionError::MalformedGeometry(id));
        }
        Ok(())
    }

    fn compute_bbox(feature: &mut geojson::Feature, geometry: &PolygonType) -> Bbox {
        feature.bbox.take().unwrap_or_else(|| {
            let maybe_rect = create_geo_polygon(geometry)
                .bounding_rect()
                .expect("Expect a bounding rectangle");
            vec![
                maybe_rect.min().x,
                maybe_rect.min().y,
                maybe_rect.max().x,
                maybe_rect.max().y,
            ]
        })
    }

    fn create_self(feature: geojson::Feature, bbox: Bbox, geometry: PolygonType) -> PolygonFeature {
        PolygonFeature {
            bbox,
            id: feature.id,
            polygon: geometry,
            properties: feature.properties,
            foreign_members: feature.foreign_members,
        }
    }
}

impl<'a> GetBbox<'a> for PolygonFeature {
    fn bbox(&'a self) -> &'a Bbox {
        &self.bbox
    }
}

impl RTreeObject for PolygonFeature {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        <Self as GetBbox>::envelope(self)
    }
}

impl PointDistance for PolygonFeature {
    fn distance_2(
        &self,
        point: &<Self::Envelope as Envelope>::Point,
    ) -> <<Self::Envelope as Envelope>::Point as Point>::Scalar {
        let p: geo::Point<f64> = (*point).into();
        self.geo_polygon().euclidean_distance(&p).powi(2)
    }
}

#[allow(clippy::ptr_arg)]
pub fn create_geo_coordinate<T>(point_type: &PointType) -> Coord<T>
where
    T: Float + std::fmt::Debug,
{
    Coord {
        x: T::from(point_type[0]).unwrap(),
        y: T::from(point_type[1]).unwrap(),
    }
}

#[allow(clippy::ptr_arg)]
pub fn create_geo_line_string<T>(line_type: &LineStringType) -> LineString<T>
where
    T: Float + std::fmt::Debug,
{
    LineString(
        line_type
            .iter()
            .map(|point_type| create_geo_coordinate(point_type))
            .collect(),
    )
}

#[allow(clippy::ptr_arg)]
fn create_geo_polygon<T>(polygon_type: &PolygonType) -> Polygon<T>
where
    T: Float + std::fmt::Debug,
{
    let exterior = polygon_type
        .get(0)
        .map(|e| create_geo_line_string(e))
        .unwrap_or_else(|| create_geo_line_string(&vec![]));

    let interiors = if polygon_type.len() < 2 {
        vec![]
    } else {
        polygon_type[1..]
            .iter()
            .map(|line_string_type| create_geo_line_string(line_string_type))
            .collect()
    };

    Polygon::new(exterior, interiors)
}

trait GenericFeature<U, G> {
    fn take_geometry_type(feature: &mut geojson::Feature) -> Result<G, GeoJsonConversionError>;

    fn check_geometry(
        geometry: &G,
        feature: &geojson::Feature,
    ) -> Result<(), GeoJsonConversionError>;

    fn compute_bbox(feature: &mut geojson::Feature, geometry: &G) -> Bbox;

    fn create_self(feature: geojson::Feature, bbox: Bbox, geometry: G) -> U;

    fn try_from(mut feature: geojson::Feature) -> Result<U, GeoJsonConversionError> {
        let geometry = Self::take_geometry_type(&mut feature)?;

        Self::check_geometry(&geometry, &feature)?;

        let bbox = feature
            .bbox
            .take()
            .unwrap_or_else(|| Self::compute_bbox(&mut feature, &geometry));

        Ok(Self::create_self(feature, bbox, geometry))
    }
}

trait GetBbox<'a> {
    fn bbox(&'a self) -> &'a Bbox;

    fn envelope(&'a self) -> AABB<[f64; 2]> {
        AABB::from_points(
            [
                [
                    *self.bbox().first().expect("A bounding box has 4 values"),
                    *self.bbox().get(1).expect("A bounding box has 4 values"),
                ],
                [
                    *self.bbox().get(2).expect("A bounding box has 4 values"),
                    *self.bbox().get(3).expect("A bounding box has 4 values"),
                ],
            ]
            .iter(),
        )
    }
}

/// An error that results from failing to convert the `GeoJson` `Feature` to
/// a `PointFeature`, `LinestringFeature`, `PolygonFeature`, etc.
#[derive(Debug)]
pub enum GeoJsonConversionError {
    /// The Geometry is missing so no conversion can be made.
    MissingGeometry(Option<Id>),
    /// The Geometry Value variant is wrong for this type.
    IncorrectGeometryValue(String),
    /// The Geometry is malformed, such as a Point has 5 f64s
    MalformedGeometry(Option<Id>),
}
