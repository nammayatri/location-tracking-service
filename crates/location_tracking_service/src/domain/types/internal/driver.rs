use serde::{Serialize, Deserialize};
use crate::common::types::*;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriverDetailsRequest {
    pub driver_id: String,
    pub driver_mode: DriverMode,
}