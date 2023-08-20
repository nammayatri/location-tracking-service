use crate::common::types::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateDriverLocationRequest {
    pub pt: Point,
    pub ts: TimeStamp,
    pub acc: Accuracy,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BulkDataReq {
    pub ride_id: String,
    pub loc: Vec<Point>,
    pub driver_id: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DurationStruct {
    pub dur: Duration,
}