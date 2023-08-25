let location_redis_cfg = {
    redis_host = "localhost",
    redis_port = 6379,
    redis_pool_size = 10,
}

let generic_redis_cfg = {
    redis_host = "localhost",
    redis_port = 6379,
    redis_pool_size = 10,
}

let kafakCfg = {
    kafka_key = "bootstrap.servers",
    kafka_host = "localhost:9092"
}

in {
    location_redis_cfg,
    generic_redis_cfg,
    kafka_cfg = kafakCfg,
    port = 8081,
    auth_url = "http://127.0.0.1:8016/internal/auth",
    auth_api_key = "ae288466-2add-11ee-be56-0242ac120002",
    bulk_location_callback_url = "http://127.0.0.1:8016/internal/bulkLocUpdate",
    auth_token_expiry = 86400,
    min_location_accuracy = 50,
    bucket_expiry = 60,
    redis_expiry = 86400,
    last_location_timstamp_expiry = 90,
    location_update_limit = 10,
    location_update_interval = 60,
    driver_location_update_topic = "location-updates",
    driver_location_update_key = "loc",
    batch_size = 100,
}