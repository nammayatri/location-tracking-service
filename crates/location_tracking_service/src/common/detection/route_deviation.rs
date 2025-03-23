/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::{
    detection::{DetectionConfig, DetectionContext, DetectionHandler, DetectionResult},
    types::*,
    utils::distance_between_in_meters,
};
use crate::environment::RouteDeviationConfig;

pub struct RouteDeviationHandler {
    config: DetectionConfig,
    route_deviation_config: RouteDeviationConfig,
}

impl RouteDeviationHandler {
    pub fn new(config: DetectionConfig, route_deviation_config: RouteDeviationConfig) -> Self {
        Self {
            config,
            route_deviation_config,
        }
    }

    fn decode_polyline(&self, encoded: &str) -> Vec<Point> {
        let mut points = Vec::new();
        let mut index = 0;
        let mut lat = 0.0;
        let mut lng = 0.0;

        while index < encoded.len() {
            let mut shift = 0;
            let mut result = 0;

            // Extract latitude
            loop {
                let byte = encoded.as_bytes()[index] as u32 - 63;
                index += 1;
                result |= (byte & 0x1F) << shift;
                shift += 5;
                if byte < 0x20 {
                    break;
                }
            }
            let dlat = if result & 1 == 1 {
                -((result >> 1) as f64)
            } else {
                (result >> 1) as f64
            } / 100000.0;
            lat += dlat;

            // Extract longitude
            shift = 0;
            result = 0;
            loop {
                let byte = encoded.as_bytes()[index] as u32 - 63;
                index += 1;
                result |= (byte & 0x1F) << shift;
                shift += 5;
                if byte < 0x20 {
                    break;
                }
            }
            let dlng = if result & 1 == 1 {
                -((result >> 1) as f64)
            } else {
                (result >> 1) as f64
            } / 100000.0;
            lng += dlng;

            points.push(Point {
                lat: Latitude(lat),
                lon: Longitude(lng),
            });
        }
        points
    }

    fn find_closest_point_on_polyline(
        &self,
        polyline: &[Point],
        current_location: &Point,
    ) -> Point {
        let mut min_distance = f64::MAX;
        let mut closest_point = current_location.clone();

        for i in 0..polyline.len() - 1 {
            let p1 = &polyline[i];
            let p2 = &polyline[i + 1];

            let closest = self.find_closest_point_on_segment(p1, p2, current_location);
            let distance = distance_between_in_meters(&closest, current_location);

            if distance < min_distance {
                min_distance = distance;
                closest_point = closest;
            }
        }

        closest_point
    }

    fn find_closest_point_on_segment(&self, p1: &Point, p2: &Point, p: &Point) -> Point {
        let x = p.lon.inner();
        let y = p.lat.inner();
        let x1 = p1.lon.inner();
        let y1 = p1.lat.inner();
        let x2 = p2.lon.inner();
        let y2 = p2.lat.inner();

        let a = x - x1;
        let b = y - y1;
        let c = x2 - x1;
        let d = y2 - y1;

        let dot = a * c + b * d;
        let len_sq = c * c + d * d;
        let mut param = -1.0;

        if len_sq != 0.0 {
            param = dot / len_sq;
        }

        let xx;
        let yy;

        if param < 0.0 {
            xx = x1;
            yy = y1;
        } else if param > 1.0 {
            xx = x2;
            yy = y2;
        } else {
            xx = x1 + param * c;
            yy = y1 + param * d;
        }

        Point {
            lat: Latitude(yy),
            lon: Longitude(xx),
        }
    }

    pub fn check(&mut self, context: &DetectionContext) -> Option<DetectionResult> {
        if !self.config.enabled {
            return None;
        }

        // Get polyline from ride info
        let polyline = match &context.ride_info {
            Some(RideInfo::Bus { polyline, .. }) => polyline.as_ref()?,
            _ => return None, // No polyline available for other ride types
        };

        // Decode polyline into points
        let points = self.decode_polyline(polyline);
        if points.len() < 2 {
            return None;
        }

        // Find closest point on polyline and calculate deviation
        let closest_point = self.find_closest_point_on_polyline(&points, &context.location);
        let deviation = distance_between_in_meters(&closest_point, &context.location);

        // Check if deviation exceeds threshold
        if deviation > self.route_deviation_config.route_deviation_threshold_meters as f64 {
            Some(DetectionResult::RouteDeviationDetected {
                location: context.location.clone(),
                timestamp: context.timestamp,
                deviation,
            })
        } else {
            None
        }
    }
}

impl DetectionHandler for RouteDeviationHandler {
    fn name(&self) -> &'static str {
        "route_deviation"
    }

    fn is_enabled(&self, context: &DetectionContext) -> bool {
        self.config.enabled && context.ride_info.is_some()
    }

    fn check(&mut self, context: &DetectionContext) -> Option<DetectionResult> {
        self.check(context)
    }
}
