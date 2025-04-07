let redis_cfg = {
    redis_host = "0.0.0.0",
    redis_port = 6380,
    redis_pool_size = 10,
    redis_partition = 0,
    reconnect_max_attempts = 10,
    reconnect_delay = 5000,
    default_ttl = 3600,
    default_hash_ttl = 3600,
    stream_read_count = 100,
}
let replica_redis_cfg = {
    redis_host = "0.0.0.0",
    redis_port = 6380,
    redis_pool_size = 10,
    redis_partition = 0,
    reconnect_max_attempts = 10,
    reconnect_delay = 5000,
    default_ttl = 3600,
    default_hash_ttl = 3600,
    stream_read_count = 100,
}
let zone_to_redis_replica_mapping =
        { ap-south-1a = "0.0.0.0"
        , ap-south-1b = "0.0.0.0"
        , ap-south-1c = "0.0.0.0"
        }
let kafka_cfg = {
    kafka_key = "bootstrap.servers",
    kafka_host = "0.0.0.0:9092"
}
let LogLevel = < TRACE | DEBUG | INFO | WARN | ERROR | OFF >
let logger_cfg = {
    level = LogLevel.DEBUG,
    log_to_file = False
}
let stop_detection_config = {
    stop_detection_update_callback_url = "http://127.0.0.1:8016/internal/stopDetection",
    max_eligible_stop_speed_threshold = 2.0,
    radius_threshold_meters = 25,
    min_points_within_radius_threshold = 5,
    enable_onride_stop_detection = False
}
let route_deviation_config = {
    route_deviation_update_callback_url = "http://127.0.0.1:8016/internal/routeDeviation",
    route_deviation_threshold_meters = 100,
    detection_interval = 30,
    enabled = True
}
let overspeeding_config = {
    update_callback_url = "http://127.0.0.1:8016/internal/overspeeding",
    speed_limit = 27.78,
    buffer_percentage = 10.0,
    detection_interval = 30
}
-- drainer_delay :: 4 * 1024KB * 1024MB * 1024GB / 100 Bytes = 41943040
let stoppedDetectionConfig = {
    batch_count = 10,
    sample_size = 300,
    max_eligible_speed = Some 2,
    max_eligible_distance = 25
  }
let routeDeviationDetectionConfig = {
    deviation_threshold = 500,
    sample_size = 5
  }
let overspeedingDetectionConfig = {
    speed_limit = 60.0,
    sample_size = 6,
    batch_count = 3
  }
let stoppedAntiDetectionConfig = {
    batch_count = 10,
    sample_size = 300,
    max_eligible_speed = Some 2,
    max_eligible_distance = 50
  }
let routeDeviationAntiDetectionConfig = {
    deviation_threshold = 300,
    sample_size = 5
  }
let overspeedingAntiDetectionConfig = {
    speed_limit = 50.0,
    sample_size = 6,
    batch_count = 3
  }
let stoppedDetectionConfigT = { max_eligible_distance : Natural, max_eligible_speed : Optional Natural, batch_count : Natural, sample_size : Natural }
let routeDeviationDetectionConfigT = { deviation_threshold : Natural, sample_size : Natural}
let overspeedingDetectionConfigT = { sample_size : Natural, speed_limit : Double, batch_count : Natural }
let DetectionConfigType =
      < StoppedDetection : stoppedDetectionConfigT | RouteDeviationDetection : routeDeviationDetectionConfigT | OverspeedingDetection : overspeedingDetectionConfigT >
let detection_violation_cab_config = {=}
  with Stopped = {
    enabled_on_pick_up = False,
    enabled_on_ride = False,
    detection_config = DetectionConfigType.StoppedDetection stoppedDetectionConfig
  }
let detection_violation_bus_config = {=}
  with Stopped = {
    enabled_on_pick_up = True,
    enabled_on_ride = True,
    detection_config = DetectionConfigType.StoppedDetection stoppedDetectionConfig
  }
  with RouteDeviation = {
    enabled_on_pick_up = True,
    enabled_on_ride = True,
    detection_config = DetectionConfigType.RouteDeviationDetection routeDeviationDetectionConfig
  }
  with Overspeeding = {
    enabled_on_pick_up = True,
    enabled_on_ride = True,
    detection_config = DetectionConfigType.OverspeedingDetection overspeedingDetectionConfig
  }
let detection_anti_violation_cab_config = {=}
  with Stopped = {
    enabled_on_pick_up = False,
    enabled_on_ride = False,
    detection_config = DetectionConfigType.StoppedDetection stoppedAntiDetectionConfig
  }
let detection_anti_violation_bus_config = {=}
  with Stopped = {
    enabled_on_pick_up = True,
    enabled_on_ride = True,
    detection_config = DetectionConfigType.StoppedDetection stoppedAntiDetectionConfig
  }
  with RouteDeviation = {
    enabled_on_pick_up = True,
    enabled_on_ride = True,
    detection_config = DetectionConfigType.RouteDeviationDetection routeDeviationAntiDetectionConfig
  }
  with Overspeeding = {
    enabled_on_pick_up = True,
    enabled_on_ride = True,
    detection_config = DetectionConfigType.OverspeedingDetection overspeedingAntiDetectionConfig
  }
let detection_violation_config = {=}
  with SEDAN = detection_violation_cab_config
  with BUS_AC = detection_violation_bus_config
let detection_anti_violation_config = {=}
  with SEDAN = detection_anti_violation_cab_config
  with BUS_AC = detection_anti_violation_bus_config
in {
    logger_cfg = logger_cfg,
    redis_cfg = redis_cfg,
    replica_redis_cfg = Some replica_redis_cfg,
    zone_to_redis_replica_mapping = Some zone_to_redis_replica_mapping,
    workers = 1,
    drainer_size = 10,
    drainer_delay = 20,
    kafka_cfg = kafka_cfg,
    port = 8081,
    auth_url = "http://127.0.0.1:8016/internal/auth",
    auth_api_key = "ae288466-2add-11ee-be56-0242ac120002",
    bulk_location_callback_url = "http://127.0.0.1:8016/internal/bulkLocUpdate",
    stop_detection = stop_detection_config,
    auth_token_expiry = 86400,
    min_location_accuracy = 50.0,
    driver_location_accuracy_buffer = 25.0,
    driver_reached_destination_buffer = 25.0,
    driver_reached_destination_callback_url = "http://127.0.0.1:8016/internal/destinationReached",
    redis_expiry = 86400,
    last_location_timstamp_expiry = 86400,
    location_update_limit = 6000000000,
    location_update_interval = 60,
    driver_location_update_topic = "location-updates",
    batch_size = 100,
    bucket_size = 300,
    nearby_bucket_threshold = 4,
    blacklist_merchants = ["favorit0-0000-0000-0000-00000favorit"],
    request_timeout = 9000,
    log_unprocessible_req_body = ["UNPROCESSIBLE_REQUEST", "REQUEST_TIMEOUT", "LARGE_PAYLOAD_SIZE", "HITS_LIMIT_EXCEEDED"],
    max_allowed_req_size = 512000, -- 500 KB
    driver_location_delay_in_sec = 60,
    driver_location_delay_for_new_ride_sec = 10,
    trigger_fcm_callback_url = "http://127.0.0.1:8016/internal/driverInactiveFCM",
    trigger_fcm_callback_url_bap = "http://127.0.0.1:8013/internal/driverArrivalNotification",
    apns_url = "https://api.sandbox.push.apple.com:443",
    pickup_notification_threshold = 40.0,
    arriving_notification_threshold = 100.0,
    detection_callback_url = "http://127.0.0.1:8016/internal/violationDetection",
    detection_violation_config = detection_violation_config,
    detection_anti_violation_config = detection_anti_violation_config
}