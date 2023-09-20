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
