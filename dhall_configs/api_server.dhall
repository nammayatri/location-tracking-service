let redisCfg = {
    redis_host = "localhost",
    redis_port = 6379,
}

let kafakCfg = {
    kafka_key = "bootstrap.servers",
    kafka_host = "localhost:9092"
}

in {
    redis_cfg = redisCfg,
    kafka_cfg = kafakCfg,
    port = 8082,
    auth_url = "",
    token_expiry = 86400,
    location_expiry = 60,
    on_ride_expiry = 86400,
    test_location_expiry = 90,
    location_update_limit = 10,
    location_update_interval = 60,
    driver_location_update_topic = "location-updates",
    driver_location_update_key = "loc"
}