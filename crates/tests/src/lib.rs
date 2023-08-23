use shared::redis::interface::types::{RedisConnectionPool, RedisSettings};

#[test]
fn test_read_geo_polygon() {
    print!("helloworld!");
}

#[tokio::test]
async fn test_set_key() {
    let is_success = tokio::task::spawn_blocking(move || {
        futures::executor::block_on(async {
            let pool = RedisConnectionPool::new(&RedisSettings::default())
                .await
                .expect("Failed to create Redis Connection Pool");
            let result = pool.set_key("helloworld!", "value".to_string()).await;
            print!("{:?}", pool.get_key::<String>("helloworld!").await);
            result.is_ok()
        })
    })
    .await
    .expect("Spawn block failure");
    assert!(is_success);
}
