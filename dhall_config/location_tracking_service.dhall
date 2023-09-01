let non_persistent_redis_cfg = {
    redis_host = "0.0.0.0",
    redis_port = 6380,
    redis_pool_size = 10,
    redis_partition = 2
}

let persistent_redis_cfg = {
    redis_host = "0.0.0.0",
    redis_port = 6381,
    redis_pool_size = 10,
    redis_partition = 3
}

let kafkaCfg = {
    kafka_key = "bootstrap.servers",
    kafka_host = "0.0.0.0:29092"
}

in {
    non_persistent_redis_cfg,
    persistent_redis_cfg,
    drainer_delay = 10,
    kafka_cfg = kafkaCfg,
    port = 8081,
    auth_url = "http://127.0.0.1:8016/internal/auth",
    auth_api_key = "ae288466-2add-11ee-be56-0242ac120002",
    bulk_location_callback_url = "http://127.0.0.1:8016/internal/bulkLocUpdate",
    auth_token_expiry = 86400,
    min_location_accuracy = 50,
    bucket_expiry = 60,
    redis_expiry = 86400,
    last_location_timstamp_expiry = 86400,
    location_update_limit = 5,
    location_update_interval = 60,
    driver_location_update_topic = "location-updates",
    driver_location_update_key = "loc",
    batch_size = 100,
    bucket_size = 30,
    nearby_bucket_threshold = 2,
}