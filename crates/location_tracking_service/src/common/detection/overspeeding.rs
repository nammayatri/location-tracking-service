/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::detection::{
    DetectionConfig, DetectionContext, DetectionHandler, DetectionResult,
};
use crate::common::types::*;

pub struct OverspeedingHandler {
    config: DetectionConfig,
    speed_limit: f64,
    buffer_percentage: f64,
}

impl OverspeedingHandler {
    pub fn new(config: DetectionConfig, speed_limit: f64, buffer_percentage: f64) -> Self {
        Self {
            config,
            speed_limit,
            buffer_percentage,
        }
    }

    fn is_overspeeding(&self, speed: Option<SpeedInMeterPerSecond>) -> bool {
        if let Some(speed) = speed {
            let speed_value = speed.inner();
            let max_allowed_speed = self.speed_limit * (1.0 + self.buffer_percentage / 100.0);
            speed_value > max_allowed_speed
        } else {
            false
        }
    }
}

impl DetectionHandler for OverspeedingHandler {
    fn name(&self) -> &'static str {
        "overspeeding"
    }

    fn is_enabled(&self, _context: &DetectionContext) -> bool {
        self.config.enabled
    }

    fn check(&mut self, context: &DetectionContext) -> Option<DetectionResult> {
        if let Some(speed) = context.speed {
            if self.is_overspeeding(Some(speed)) {
                return Some(DetectionResult::OverspeedingDetected {
                    location: context.location.clone(),
                    timestamp: context.timestamp,
                    speed: speed.inner(),
                    speed_limit: self.speed_limit,
                });
            }
        }
        None
    }
}
