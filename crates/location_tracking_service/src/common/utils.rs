use geo::{point, Intersects};
use super::{errors::AppError, types::*};

pub fn get_city(lat: Latitude, lon: Longitude, polygon: Vec<MultiPolygonBody>) -> Result<String, AppError> {
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