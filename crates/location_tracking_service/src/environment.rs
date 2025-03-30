/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
#![allow(clippy::expect_used)]

use std::{env::var, sync::Arc};

use rdkafka::{error::KafkaError, producer::FutureProducer, ClientConfig};
use reqwest::Url;
use serde::Deserialize;
use shared::redis::types::{RedisConnectionPool, RedisSettings};
use std::collections::HashMap;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::common::{geo_polygon::read_geo_polygon, types::*};

use shared::tools::logger::LoggerConfig;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub port: u16,
    pub logger_cfg: LoggerConfig,
    pub redis_cfg: RedisConfig,
    pub replica_redis_cfg: Option<RedisConfig>,
    pub zone_to_redis_replica_mapping: Option<HashMap<String, String>>,
    pub workers: usize,
    pub drainer_delay: u64,
    pub drainer_size: usize,
    pub auth_url: String,
    pub auth_api_key: String,
    pub bulk_location_callback_url: String,
    pub auth_token_expiry: u32,
    pub redis_expiry: u32,
    pub min_location_accuracy: f64,
    pub last_location_timstamp_expiry: u32,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub stop_detection: StopDetectionConfig,
    pub stop_detection_vehicle_configs: Option<HashMap<VehicleType, StopDetectionConfig>>,
    pub route_deviation: RouteDeviationConfig,
    pub route_deviation_vehicle_configs: Option<HashMap<VehicleType, RouteDeviationConfig>>,
    pub overspeeding: OverspeedingConfig,
    pub overspeeding_vehicle_configs: Option<HashMap<VehicleType, OverspeedingConfig>>,
    pub kafka_cfg: KafkaConfig,
    pub driver_location_update_topic: String,
    pub batch_size: i64,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
    pub driver_location_accuracy_buffer: f64,
    pub driver_reached_destination_buffer: f64,
    pub pickup_notification_threshold: f64,
    pub arriving_notification_threshold: f64,
    #[serde(deserialize_with = "deserialize_url")]
    pub driver_reached_destination_callback_url: Url,
    pub blacklist_merchants: Vec<String>,
    pub request_timeout: u64,
    pub log_unprocessible_req_body: Vec<String>,
    pub max_allowed_req_size: usize,
    pub driver_location_delay_in_sec: i64,
    pub driver_location_delay_for_new_ride_sec: i64,
    pub trigger_fcm_callback_url: String,
    pub trigger_fcm_callback_url_bap: String,
    pub apns_url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub kafka_key: String,
    pub kafka_host: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub redis_host: String,
    pub redis_port: u16,
    pub redis_pool_size: usize,
    pub redis_partition: usize,
    pub reconnect_max_attempts: u32,
    pub reconnect_delay: u32,
    pub default_ttl: u32,
    pub default_hash_ttl: u32,
    pub stream_read_count: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StopDetectionConfig {
    #[serde(deserialize_with = "deserialize_url")]
    pub stop_detection_update_callback_url: Url,
    pub max_eligible_stop_speed_threshold: f64,
    pub radius_threshold_meters: u64,
    pub min_points_within_radius_threshold: usize,
    pub enable_onride_stop_detection: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouteDeviationConfig {
    #[serde(deserialize_with = "deserialize_url")]
    pub route_deviation_update_callback_url: Url,
    pub route_deviation_threshold_meters: u64,
    pub detection_interval: u64,
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OverspeedingConfig {
    #[serde(deserialize_with = "deserialize_url")]
    pub update_callback_url: Url,
    pub speed_limit: f64,
    pub buffer_percentage: f64,
    pub detection_interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetectionConfigs {
    pub stop_detection: StopDetectionConfigs,
    pub route_deviation: RouteDeviationConfigs,
    pub overspeeding: OverspeedingConfigs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetectionConfig {
    pub enabled: bool,
    #[serde(deserialize_with = "deserialize_url")]
    pub update_callback_url: Url,
    pub detection_interval: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StopDetectionConfigs {
    pub default: StopDetectionConfig,
    pub vehicle_specific: HashMap<VehicleType, StopDetectionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteDeviationConfigs {
    pub default: RouteDeviationConfig,
    pub vehicle_specific: HashMap<VehicleType, RouteDeviationConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverspeedingConfigs {
    pub default: OverspeedingConfig,
    pub vehicle_specific: HashMap<VehicleType, OverspeedingConfig>,
}

fn deserialize_url<'de, D>(deserializer: D) -> Result<Url, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Url::parse(&s).map_err(serde::de::Error::custom)
}

#[derive(Clone)]
pub struct AppState {
    pub redis: Arc<RedisConnectionPool>,
    pub sender: Sender<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    pub drainer_delay: u64,
    pub drainer_size: usize,
    pub polygon: Vec<MultiPolygonBody>,
    pub blacklist_polygon: Vec<MultiPolygonBody>,
    pub auth_url: Url,
    pub auth_api_key: String,
    pub bulk_location_callback_url: Url,
    pub auth_token_expiry: u32,
    pub redis_expiry: u32,
    pub min_location_accuracy: Accuracy,
    pub stop_detection: StopDetectionConfig,
    pub last_location_timstamp_expiry: u32,
    pub location_update_limit: usize,
    pub location_update_interval: u64,
    pub producer: Option<FutureProducer>,
    pub driver_location_update_topic: String,
    pub batch_size: i64,
    pub bucket_size: u64,
    pub nearby_bucket_threshold: u64,
    pub driver_location_accuracy_buffer: f64,
    pub driver_reached_destination_buffer: f64,
    pub driver_reached_destination_callback_url: Url,
    pub blacklist_merchants: Vec<MerchantId>,
    pub max_allowed_req_size: usize,
    pub log_unprocessible_req_body: Vec<String>,
    pub request_timeout: u64,
    pub driver_location_delay_in_sec: i64,
    pub driver_location_delay_for_new_ride_sec: i64,
    pub trigger_fcm_callback_url: Url,
    pub trigger_fcm_callback_url_bap: Url,
    pub apns_url: Url,
    pub pickup_notification_threshold: f64,
    pub arriving_notification_threshold: f64,
    pub detection_configs: DetectionConfigs,
}

impl AppState {
    pub async fn new(
        app_config: AppConfig,
        sender: Sender<(Dimensions, Latitude, Longitude, TimeStamp, DriverId)>,
    ) -> AppState {
        let pod_zone = var("POD_ZONE").ok();
        let new_replica_host = pod_zone
            .as_ref()
            .and_then(|zone| app_config.zone_to_redis_replica_mapping.as_ref()?.get(zone))
            .cloned();

        let redis = Arc::new(
            RedisConnectionPool::new(
                RedisSettings::new(
                    app_config.redis_cfg.redis_host,
                    app_config.redis_cfg.redis_port,
                    app_config.redis_cfg.redis_pool_size,
                    app_config.redis_cfg.redis_partition,
                    app_config.redis_cfg.reconnect_max_attempts,
                    app_config.redis_cfg.reconnect_delay,
                    app_config.redis_cfg.default_ttl,
                    app_config.redis_cfg.default_hash_ttl,
                    app_config.redis_cfg.stream_read_count,
                ),
                app_config.replica_redis_cfg.map(|mut replica_redis_cfg| {
                    if let Some(host) = new_replica_host {
                        replica_redis_cfg.redis_host = host;
                    }
                    RedisSettings::new(
                        replica_redis_cfg.redis_host,
                        replica_redis_cfg.redis_port,
                        replica_redis_cfg.redis_pool_size,
                        replica_redis_cfg.redis_partition,
                        replica_redis_cfg.reconnect_max_attempts,
                        replica_redis_cfg.reconnect_delay,
                        replica_redis_cfg.default_ttl,
                        replica_redis_cfg.default_hash_ttl,
                        replica_redis_cfg.stream_read_count,
                    )
                }),
            )
            .await
            .expect("Failed to create Generic Redis connection pool"),
        );

        let geo_config_path = var("GEO_CONFIG").unwrap_or_else(|_| "./geo_config".to_string());
        let polygons = read_geo_polygon(&geo_config_path).expect("Failed to read geoJSON");

        let blacklist_geo_config_path =
            var("BLACKLIST_GEO_CONFIG").unwrap_or_else(|_| "./blacklist_geo_config".to_string());
        let blacklist_polygons = read_geo_polygon(&blacklist_geo_config_path)
            .expect("Failed to read specialzone geoJSON");

        let producer: Option<FutureProducer>;

        let result: Result<FutureProducer, KafkaError> = ClientConfig::new()
            .set(
                app_config.kafka_cfg.kafka_key,
                app_config.kafka_cfg.kafka_host,
            )
            .set("compression.type", "lz4")
            .create();

        match result {
            Ok(val) => {
                producer = Some(val);
            }
            Err(err) => {
                producer = None;
                info!(
                    tag = "[Kafka Connection]",
                    "Error connecting to kafka config: {err}"
                );
            }
        }

        let blacklist_merchants = app_config
            .blacklist_merchants
            .into_iter()
            .map(MerchantId)
            .collect::<Vec<MerchantId>>();

        AppState {
            redis,
            drainer_delay: app_config.drainer_delay,
            drainer_size: app_config.drainer_size,
            sender,
            polygon: polygons,
            blacklist_polygon: blacklist_polygons,
            auth_url: Url::parse(app_config.auth_url.as_str()).expect("Failed to parse auth_url."),
            auth_api_key: app_config.auth_api_key,
            bulk_location_callback_url: Url::parse(app_config.bulk_location_callback_url.as_str())
                .expect("Failed to parse bulk_location_callback_url."),
            auth_token_expiry: app_config.auth_token_expiry,
            min_location_accuracy: Accuracy(app_config.min_location_accuracy),
            redis_expiry: app_config.redis_expiry,
            last_location_timstamp_expiry: app_config.last_location_timstamp_expiry,
            location_update_limit: app_config.location_update_limit,
            location_update_interval: app_config.location_update_interval,
            producer,
            driver_location_update_topic: app_config.driver_location_update_topic,
            batch_size: app_config.batch_size,
            bucket_size: app_config.bucket_size,
            nearby_bucket_threshold: app_config.nearby_bucket_threshold,
            driver_location_accuracy_buffer: app_config.driver_location_accuracy_buffer,
            driver_reached_destination_buffer: app_config.driver_reached_destination_buffer,
            driver_reached_destination_callback_url: app_config
                .driver_reached_destination_callback_url,
            max_allowed_req_size: app_config.max_allowed_req_size,
            log_unprocessible_req_body: app_config.log_unprocessible_req_body,
            request_timeout: app_config.request_timeout,
            blacklist_merchants,
            stop_detection: app_config.stop_detection.clone(),
            driver_location_delay_in_sec: app_config.driver_location_delay_in_sec,
            driver_location_delay_for_new_ride_sec: app_config
                .driver_location_delay_for_new_ride_sec,
            trigger_fcm_callback_url: Url::parse(app_config.trigger_fcm_callback_url.as_str())
                .expect("Failed to parse fcm_callback_url."),
            trigger_fcm_callback_url_bap: Url::parse(
                app_config.trigger_fcm_callback_url_bap.as_str(),
            )
            .expect("Failed to parse fcm_callback_url_bap."),
            apns_url: Url::parse(app_config.apns_url.as_str()).expect("Failed to parse apns_url."),
            pickup_notification_threshold: app_config.pickup_notification_threshold,
            arriving_notification_threshold: app_config.arriving_notification_threshold,
            detection_configs: DetectionConfigs {
                stop_detection: StopDetectionConfigs {
                    default: app_config.stop_detection,
                    vehicle_specific: app_config
                        .stop_detection_vehicle_configs
                        .unwrap_or_default(),
                },
                route_deviation: RouteDeviationConfigs {
                    default: app_config.route_deviation,
                    vehicle_specific: app_config
                        .route_deviation_vehicle_configs
                        .unwrap_or_default(),
                },
                overspeeding: OverspeedingConfigs {
                    default: app_config.overspeeding,
                    vehicle_specific: app_config.overspeeding_vehicle_configs.unwrap_or_default(),
                },
            },
        }
    }

    fn get_base_vehicle_type(vehicle_type: &VehicleType) -> VehicleType {
        match vehicle_type {
            VehicleType::SEDAN
            | VehicleType::TAXI
            | VehicleType::TaxiPlus
            | VehicleType::PremiumSedan
            | VehicleType::BLACK
            | VehicleType::BlackXl
            | VehicleType::SuvPlus
            | VehicleType::HeritageCab => VehicleType::SEDAN,
            VehicleType::BusAc | VehicleType::BusNonAc => VehicleType::BusAc,
            VehicleType::AutoRickshaw | VehicleType::EvAutoRickshaw => VehicleType::AutoRickshaw,
            VehicleType::BIKE | VehicleType::DeliveryBike => VehicleType::BIKE,
            _ => VehicleType::SEDAN, // Default to SEDAN for all other types
        }
    }

    pub fn get_stop_detection_config(&self, vehicle_type: &VehicleType) -> &StopDetectionConfig {
        let base_type = Self::get_base_vehicle_type(vehicle_type);
        self.detection_configs
            .stop_detection
            .vehicle_specific
            .get(&base_type)
            .unwrap_or(&self.detection_configs.stop_detection.default)
    }

    pub fn get_route_deviation_config(&self, vehicle_type: &VehicleType) -> &RouteDeviationConfig {
        let base_type = Self::get_base_vehicle_type(vehicle_type);
        self.detection_configs
            .route_deviation
            .vehicle_specific
            .get(&base_type)
            .unwrap_or(&self.detection_configs.route_deviation.default)
    }

    pub fn get_overspeeding_config(&self, vehicle_type: &VehicleType) -> &OverspeedingConfig {
        let base_type = Self::get_base_vehicle_type(vehicle_type);
        self.detection_configs
            .overspeeding
            .vehicle_specific
            .get(&base_type)
            .unwrap_or(&self.detection_configs.overspeeding.default)
    }
}
