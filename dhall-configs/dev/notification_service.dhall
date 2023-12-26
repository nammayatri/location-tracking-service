let redis_cfg = {
    redis_host = "0.0.0.0",
    redis_port = 6379,
    redis_pool_size = 10,
    redis_partition = 0,
    reconnect_max_attempts = 10,
    reconnect_delay = 5000,
    default_ttl = 3600,
    default_hash_ttl = 3600,
    stream_read_count = 100,
}

let LogLevel = < TRACE | DEBUG | INFO | WARN | ERROR | OFF >

let logger_cfg = {
    level = LogLevel.ERROR,
    log_to_file = False
}

let kafka_cfg = {
    kafka_key = "bootstrap.servers",
    kafka_host = "0.0.0.0:29092"
}

in {
    port = 50051,
    prometheus_port = 9090,
    logger_cfg = logger_cfg,
    redis_cfg = redis_cfg,
    kafka_cfg = kafka_cfg,
    reader_delay_seconds = 1,
    retry_delay_seconds = 60,
}