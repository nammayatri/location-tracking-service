/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::common::types::*;
use crate::environment::AppConfig;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

mod overspeeding;
mod route_deviation;
mod stop;

pub use overspeeding::OverspeedingHandler;
pub use route_deviation::RouteDeviationHandler;
pub use stop::StopDetectionHandler;

/// Represents the result of a detection check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectionResult {
    StopDetected {
        location: Point,
        timestamp: TimeStamp,
    },
    RouteDeviationDetected {
        location: Point,
        timestamp: TimeStamp,
        deviation: f64,
    },
    OverspeedingDetected {
        location: Point,
        timestamp: TimeStamp,
        speed: f64,
        speed_limit: f64,
    },
}

/// Configuration for any detection type
#[derive(Debug, Clone)]
pub struct DetectionConfig {
    pub enabled: bool,
    pub update_callback_url: Url,
    pub detection_interval: i64, // in seconds
}

/// Context data needed for detection
#[derive(Debug, Clone)]
pub struct DetectionContext {
    pub driver_id: DriverId,
    pub location: Point,
    pub timestamp: TimeStamp,
    pub speed: Option<SpeedInMeterPerSecond>,
    pub ride_status: Option<RideStatus>,
    pub ride_info: Option<RideInfo>,
}

/// Trait for core detection logic
pub trait DetectionHandler: Send + Sync {
    /// Name of the detection type
    fn name(&self) -> &'static str;

    /// Check if detection is enabled for this context
    fn is_enabled(&self, context: &DetectionContext) -> bool;

    /// Perform the detection check
    fn check(&mut self, context: &DetectionContext) -> Option<DetectionResult>;
}

/// Registry to manage all detection handlers
pub struct DetectionRegistry {
    detection_handlers: HashMap<String, Arc<dyn DetectionHandler>>,
}

impl DetectionRegistry {
    pub fn new() -> Self {
        Self {
            detection_handlers: HashMap::new(),
        }
    }

    pub fn register_handler<H>(&mut self, handler: H)
    where
        H: DetectionHandler + Send + Sync + 'static,
    {
        let name = handler.name().to_string();
        let handler = Arc::new(handler);
        self.detection_handlers.insert(name, handler);
    }

    pub fn get_detection_handler(&self, name: &str) -> Option<&Arc<dyn DetectionHandler>> {
        self.detection_handlers.get(name)
    }
}

impl Default for DetectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Factory trait for creating detection handlers
pub trait DetectionHandlerFactory {
    fn create_handlers(config: &AppConfig) -> Vec<Arc<dyn DetectionHandler>>;
}
