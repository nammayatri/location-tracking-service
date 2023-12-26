/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::NotificationPayload;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use rustc_hash::FxHashMap;
use serde::de::DeserializeOwned;
use serde_json::json;

pub fn decode_nested_json<T: DeserializeOwned>(payload: Vec<(String, String)>) -> Result<T> {
    let mut json_obj = json!({});
    for (key, value) in payload {
        let parts: Vec<&str> = key.split('.').collect();
        let mut current_obj = &mut json_obj;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                current_obj[part] = serde_json::Value::String(value.to_string());
            } else {
                if !current_obj.as_object().unwrap().contains_key(*part) {
                    current_obj[part] = json!({});
                }
                current_obj = current_obj.get_mut(part).unwrap();
            }
        }
    }

    // Deserialize JSON into Rust type
    let payload = serde_json::from_value::<T>(json_obj)?;

    Ok(payload)
}

pub fn decode_notification_payload(
    notifications: FxHashMap<String, Vec<Vec<(String, String)>>>,
) -> Result<FxHashMap<String, Vec<NotificationPayload>>> {
    let mut result = FxHashMap::default();

    for (key, notifications) in notifications {
        let mut payloads = Vec::new();

        for notification in notifications {
            let payload = decode_nested_json::<NotificationPayload>(notification)?;
            payloads.push(payload);
        }

        result.insert(key, payloads);
    }

    Ok(result)
}

pub fn get_redis_stream_id_with_seconds_offset_from_current_time(
    time: DateTime<Utc>,
    seconds_offset: i64,
) -> String {
    let offset_time = time + Duration::seconds(seconds_offset);
    let unix_timestamp = offset_time.timestamp();
    format!("{}-0", unix_timestamp)
}

pub fn diff_utc(old: DateTime<Utc>, new: DateTime<Utc>) -> Duration {
    new.signed_duration_since(old)
}
