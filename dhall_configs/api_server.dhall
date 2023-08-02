let redisCfg = {
    redis_host = "localhost",
    redis_port = 6379,
}

in {
    redis_cfg = redisCfg,
    auth_url = "",
    token_expiry = 86400,
    location_expiry = 3600,
    on_ride_expiry = 86400,
}