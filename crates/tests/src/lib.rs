/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

#[test]
fn test_read_geo_polygon() {
    print!("helloworld!");
}

#[tokio::test]
async fn test_set_key() {
    use shared::redis::types::{RedisConnectionPool, RedisSettings};

    let is_success = tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async {
            let pool = RedisConnectionPool::new(RedisSettings::default(), None)
                .await
                .expect("Failed to create Redis Connection Pool");
            let result = pool.set_key("helloworld!", "value".to_string(), 3600).await;
            result.is_ok()
        })
    })
    .await
    .expect("Spawn block failure");
    assert!(is_success);
}
