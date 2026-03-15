/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use crate::{
    common::{kafka::push_to_kafka_raw, types::*},
    domain::types::ui::location::UpdateDriverLocationRequest,
    tools::prometheus::DB_WRITE_DURATION_SECONDS,
};
use rdkafka::producer::FutureProducer;
use serde_json;
use tracing::error;

/// Streams location updates for drivers to a Kafka topic.
///
/// This function iterates over a list of location updates and pushes
/// each one to a specified Kafka topic. The function also derives
/// the `ride_status` based on the provided `RideStatus`.
///
/// # Parameters
/// - `producer`: An optional Kafka producer to send messages to Kafka.
/// - `secondary_producer`: An optional secondary Kafka producer for dual-write scenarios.
/// - `topic`: The Kafka topic to which the location updates will be published.
/// - `locations`: A list of location updates for a driver.
/// - `merchant_id`: The unique identifier for the merchant.
/// - `ride_id`: An optional unique identifier for the ongoing ride.
/// - `ride_status`: The current status of the ride (e.g., NEW, INPROGRESS).
/// - `driver_mode`: The mode in which the driver is currently operating.
/// - `DriverId(key)`: The unique identifier for the driver.
///
/// # Note
/// If an error occurs while pushing a message to Kafka, the function logs
/// the error but continues processing the remaining location updates.
#[allow(clippy::too_many_arguments)]
pub async fn kafka_stream_updates(
    producer: &Option<FutureProducer>,
    secondary_producer: &Option<FutureProducer>,
    topic: &str,
    locations: Vec<(UpdateDriverLocationRequest, LocationType)>,
    server_timestamp: TimeStamp,
    merchant_id: MerchantId,
    merchant_operating_city_id: MerchantOperatingCityId,
    ride_id: Option<RideId>,
    ride_status: Option<RideStatus>,
    driver_mode: DriverMode,
    DriverId(key): &DriverId,
    vehicle_type: VehicleType,
    stop_location: Option<Point>,
    next_upcoming_stop_eta: Option<TimeStamp>,
    // travelled_distance: Meters,
) {
    let ride_status = match ride_status {
        Some(RideStatus::NEW) => DriverRideStatus::OnPickup,
        Some(RideStatus::INPROGRESS) => DriverRideStatus::OnRide,
        _ => DriverRideStatus::IDLE,
    };

    let (is_stop_detected, stop_lat, stop_lon) = if let Some(stop_location) = stop_location {
        (Some(true), Some(stop_location.lat), Some(stop_location.lon))
    } else {
        (None, None, None)
    };

    let on_ride = ride_status != DriverRideStatus::IDLE;

    // Pre-serialize all messages to avoid repeated serde work during async sends
    let serialized: Vec<_> = locations
        .into_iter()
        .filter_map(|(loc, location_type)| {
            let message = LocationUpdate {
                r_id: ride_id.clone(),
                m_id: merchant_id.clone(),
                pt: Point {
                    lat: loc.pt.lat,
                    lon: loc.pt.lon,
                },
                da: true,
                rid: ride_id.clone(),
                mocid: merchant_operating_city_id.clone(),
                ts: loc.ts,
                st: server_timestamp,
                lat: loc.pt.lat,
                lon: loc.pt.lon,
                speed: loc.v.unwrap_or(SpeedInMeterPerSecond(0.0)),
                acc: loc.acc.unwrap_or(Accuracy(0.0)),
                ride_status,
                on_ride,
                active: true,
                mode: driver_mode,
                bear: loc.bear,
                vehicle_variant: vehicle_type,
                is_stop_detected,
                stop_lat,
                stop_lon,
                location_type,
                next_upcoming_stop_eta,
            };
            match serde_json::to_string(&message) {
                Ok(s) => Some(s),
                Err(err) => {
                    error!("Error serializing kafka message => {}", err);
                    None
                }
            }
        })
        .collect();

    // Send all serialized messages concurrently to Kafka
    let _timer = DB_WRITE_DURATION_SECONDS
        .with_label_values(&["kafka_publish"])
        .start_timer();
    let futures: Vec<_> = serialized
        .iter()
        .map(|msg_str| {
            push_to_kafka_raw(producer, secondary_producer, topic, key.as_str(), msg_str)
        })
        .collect();

    for result in futures::future::join_all(futures).await {
        if let Err(err) = result {
            error!("Error occured in push_to_kafka => {}", err.message())
        }
    }
}
