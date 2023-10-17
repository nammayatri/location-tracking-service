/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use std::time::{SystemTime, UNIX_EPOCH};

use shared::{redis::types::RedisConnectionPool, tools::error::AppError};

pub async fn sliding_window_limiter(
    persistent_redis_pool: &RedisConnectionPool,
    key: &str,
    frame_hits_lim: usize,
    frame_len: u32,
) -> Result<Vec<i64>, AppError> {
    let curr_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as i64;

    let hits = persistent_redis_pool.get_key(key).await?;

    let hits = match hits {
        Some(hits) => serde_json::from_str::<Vec<i64>>(&hits)
            .map_err(|_| AppError::InternalError("Failed to parse hits from redis.".to_string()))?,
        None => vec![],
    };
    let (filt_hits, ret) = sliding_window_limiter_pure(curr_time, hits, frame_hits_lim, frame_len);

    if !ret {
        return Err(AppError::HitsLimitExceeded(key.to_string()));
    }

    let _ = persistent_redis_pool
        .set_key(
            key,
            serde_json::to_string(&filt_hits).expect("Failed to parse filt_hits to string."),
            frame_len,
        )
        .await;

    Ok(filt_hits)
}

fn sliding_window_limiter_pure(
    curr_time: i64,
    hits: Vec<i64>,
    frame_hits_lim: usize,
    frame_len: u32,
) -> (Vec<i64>, bool) {
    let curr_frame = get_time_frame(curr_time, frame_len);
    let filt_hits = hits
        .into_iter()
        .filter(|hit| hits_filter(curr_frame, *hit))
        .collect::<Vec<_>>();
    let prev_frame_hits_len = filt_hits
        .iter()
        .filter(|hit| prev_frame_hits_filter(curr_frame, hit))
        .count();
    let prev_frame_weight = 1.0 - (curr_time as f64 % frame_len as f64) / frame_len as f64;
    let curr_frame_hits_len: i32 = filt_hits
        .iter()
        .filter(|hit| curr_frame_hits_filter(curr_frame, hit))
        .count() as i32;

    let res = (prev_frame_hits_len as f64 * prev_frame_weight) as i32 + curr_frame_hits_len
        < frame_hits_lim as i32;

    (
        if res {
            let mut new_hits = Vec::with_capacity(filt_hits.len() + 1);
            new_hits.push(curr_frame);
            new_hits.extend(filt_hits);
            new_hits
        } else {
            filt_hits
        },
        res,
    )
}

fn get_time_frame(time: i64, frame_len: u32) -> i64 {
    time / frame_len as i64
}

fn hits_filter(curr_frame: i64, time_frame: i64) -> bool {
    time_frame == curr_frame - 1 || time_frame == curr_frame
}

fn prev_frame_hits_filter(curr_frame: i64, time_frame: &i64) -> bool {
    *time_frame == curr_frame - 1
}

fn curr_frame_hits_filter(curr_frame: i64, time_frame: &i64) -> bool {
    *time_frame == curr_frame
}
