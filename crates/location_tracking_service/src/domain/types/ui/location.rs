use crate::common::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
#[derive(Serialize)]
pub struct LocationUpdate {
    pub r_id: String,
    pub m_id: String,
    pub ts: DateTime<Utc>,
    pub st: DateTime<Utc>,
    pub pt: Point,
    pub acc: i32,
    pub ride_status: String,
    pub da: bool,
    pub mode: String,
}
