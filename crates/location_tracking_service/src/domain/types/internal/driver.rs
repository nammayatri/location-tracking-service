use crate::common::types::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverDetailsRequest {
    pub driver_id: String,
    pub driver_mode: DriverMode,
}
