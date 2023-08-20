let location_redis_cfg = {
    redis_host = "localhost",
    redis_port = 6379,
}

let generic_redis_cfg = {
    redis_host = "localhost",
    redis_port = 6380,
}

in {
    location_redis_cfg,
    generic_redis_cfg,
    port = 9090,
    auth_url = "",
    token_expiry = 86400,
    location_expiry = 60,
    on_ride_expiry = 86400,
    test_location_expiry = 90,
    location_update_limit = 10,
    location_update_interval = 60,
}