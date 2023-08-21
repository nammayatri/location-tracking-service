let location_redis_cfg = {
    redis_host = "localhost",
    redis_port = 6379,
    redis_pool_size = 10,
}

let generic_redis_cfg = {
    redis_host = "localhost",
    redis_port = 6380,
    redis_pool_size = 10,
}

in {
    location_redis_cfg,
    generic_redis_cfg,
    port = 8081,
    auth_url = "http://127.0.0.1:8016/internal/auth",
    bulk_location_callback_url = "http://127.0.0.1:8016/internal/bulkLocUpdate",
    token_expiry = 86400,
    location_expiry = 60,
    on_ride_expiry = 86400,
    test_location_expiry = 90,
    location_update_limit = 10,
    location_update_interval = 60,
}