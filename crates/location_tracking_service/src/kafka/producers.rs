/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use crate::{
    common::{kafka::push_to_kafka, types::*},
    domain::types::ui::location::UpdateDriverLocationRequest,
};
use chrono::Utc;
use log::*;
use rdkafka::producer::FutureProducer;

/// Streams location updates for drivers to a Kafka topic.
///
/// This function iterates over a list of location updates and pushes
/// each one to a specified Kafka topic. The function also derives
/// the `ride_status` based on the provided `RideStatus`.
///
/// # Parameters
/// - `producer`: An optional Kafka producer to send messages to Kafka.
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
    topic: &str,
    locations: Vec<UpdateDriverLocationRequest>,
    merchant_id: MerchantId,
    merchant_operating_city_id: MerchantOperatingCityId,
    ride_id: Option<RideId>,
    ride_status: Option<RideStatus>,
    driver_mode: DriverMode,
    DriverId(key): &DriverId,
    vehicle_type: VehicleType,
    stop_detected: Option<(Point, usize)>,
    // travelled_distance: Meters,
) {
    let ride_status = match ride_status {
        Some(RideStatus::NEW) => DriverRideStatus::OnPickup,
        Some(RideStatus::INPROGRESS) => DriverRideStatus::OnRide,
        _ => DriverRideStatus::IDLE,
    };

    let (is_stop_detected, stop_lat, stop_lon, stop_points) =
        if let Some((stop_mean_location, stop_total_points)) = stop_detected {
            (
                Some(true),
                Some(stop_mean_location.lat),
                Some(stop_mean_location.lon),
                Some(stop_total_points),
            )
        } else {
            (None, None, None, None)
        };

    for loc in locations {
        let message = LocationUpdate {
            r_id: ride_id.to_owned(),
            m_id: merchant_id.to_owned(),
            pt: Point {
                lat: loc.pt.lat,
                lon: loc.pt.lon,
            },
            da: true,
            rid: ride_id.to_owned(),
            mocid: merchant_operating_city_id.to_owned(),
            ts: loc.ts,
            st: TimeStamp(Utc::now()),
            lat: loc.pt.lat,
            lon: loc.pt.lon,
            speed: loc.v.unwrap_or(SpeedInMeterPerSecond(0.0)),
            acc: loc.acc.unwrap_or(Accuracy(0.0)),
            ride_status: ride_status.to_owned(),
            on_ride: ride_status != DriverRideStatus::IDLE,
            active: true,
            mode: driver_mode.to_owned(),
            vehicle_variant: vehicle_type.to_owned(),
            is_stop_detected,
            stop_lat,
            stop_lon,
            stop_points,
            // travelled_distance: travelled_distance.to_owned(),
        };
        if let Err(err) = push_to_kafka(producer, topic, key.as_str(), message).await {
            error!("Error occured in push_to_kafka => {}", err.message())
        }
    }
}
