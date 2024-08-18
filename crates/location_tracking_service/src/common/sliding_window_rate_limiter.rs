/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::tools::error::AppError;
use chrono::Utc;
use shared::redis::types::RedisConnectionPool;

/// Applies a sliding window rate limiter using a Redis backend.
///
/// This function implements a sliding window rate limiter that checks
/// how many hits have occurred in the current and previous time frames
/// and determines if the rate limit has been exceeded.
///
/// # Parameters
/// - `redis`: The Redis connection pool.
/// - `key`: The Redis key to use for tracking hits.
/// - `frame_hits_lim`: The maximum number of hits allowed in a time frame.
/// - `frame_len`: The length of a time frame in seconds.
///
/// # Returns
/// A vector of timestamps representing hits if within the limit, or an error otherwise.
pub async fn sliding_window_limiter(
    redis: &RedisConnectionPool,
    key: &str,
    frame_hits_lim: usize,
    frame_len: u32,
) -> Result<(), AppError> {
    let curr_time = Utc::now().timestamp();

    let hits_frame = redis
        .get_key::<Vec<i64>>(key)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?
        .unwrap_or(Vec::new());

    let curr_frame = curr_time / frame_len as i64;

    let prev_and_curr_frame_hits = hits_frame
        .into_iter()
        .filter(|hits_frame| *hits_frame == curr_frame - 1 || *hits_frame == curr_frame)
        .collect::<Vec<_>>();

    let curr_frame_hits_len: i32 = prev_and_curr_frame_hits
        .iter()
        .filter(|hits_frame| **hits_frame == curr_frame)
        .count() as i32;

    let prev_frame_hits_len = prev_and_curr_frame_hits
        .iter()
        .filter(|hits_frame| **hits_frame == curr_frame - 1)
        .count();
    let prev_frame_weight = 1.0 - (curr_time as f64 % frame_len as f64) / frame_len as f64;

    if (prev_frame_hits_len as f64 * prev_frame_weight) as i32 + curr_frame_hits_len
        < frame_hits_lim as i32
    {
        let mut new_hits = Vec::with_capacity(prev_and_curr_frame_hits.len() + 1);
        new_hits.push(curr_frame);
        new_hits.extend(prev_and_curr_frame_hits);

        redis
            .set_key(key, new_hits, frame_len)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))
    } else {
        Err(AppError::HitsLimitExceeded(key.to_string()))
    }
}
