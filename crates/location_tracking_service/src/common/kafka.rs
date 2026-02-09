/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use std::time::Duration;

use crate::tools::error::AppError;
use log::error;
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use serde::Serialize;

/// Checks if secondary Kafka producer is enabled via environment variable.
/// Returns true only if PRODUCE_SECONDARY_KAFKA is set to "true".
fn should_produce_secondary() -> bool {
    std::env::var("PRODUCE_SECONDARY_KAFKA")
        .map(|val| val == "true")
        .unwrap_or(false)
}

/// Pushes a serialized message to primary and optionally secondary Kafka topic.
///
/// This function serializes the given message into a JSON string and
/// sends it to the specified Kafka topic using the provided primary producer.
/// If secondary producer is provided and PRODUCE_SECONDARY_KAFKA env var is set to "true",
/// it also pushes to the secondary cluster (errors are logged but not returned).
///
/// # Parameters
/// - `producer`: An optional Kafka producer to send messages to Kafka.
/// - `secondary_producer`: An optional secondary Kafka producer for dual-write scenarios.
/// - `topic`: The Kafka topic to which the message will be published.
/// - `key`: A string key associated with the message for Kafka.
/// - `message`: The message to be serialized and sent to Kafka.
///
/// # Returns
/// - `Ok(())`: If the message is successfully pushed to primary Kafka.
/// - `Err(AppError)`: If there's an error during serialization or primary Kafka push.
///
/// # Type Parameters
/// - `T`: The type of the message, which must implement the `Serialize` trait.
pub async fn push_to_kafka<T>(
    producer: &Option<FutureProducer>,
    secondary_producer: &Option<FutureProducer>,
    topic: &str,
    key: &str,
    message: T,
) -> Result<(), AppError>
where
    T: Serialize,
{
    let message_str = serde_json::to_string(&message)
        .map_err(|err| AppError::SerializationError(err.to_string()))?;

    // Push to primary producer (blocking, throws on failure)
    match producer {
        Some(producer) => {
            producer
                .send(
                    FutureRecord::to(topic).key(key).payload(&message_str),
                    Timeout::After(Duration::from_secs(1)),
                )
                .await
                .map_err(|err| AppError::KafkaPushFailed(err.0.to_string()))?;
        }
        None => {
            return Err(AppError::KafkaPushFailed(
                "[Kafka] Producer is None, unable to send message".to_string(),
            ))
        }
    }

    // Push to secondary producer if enabled (fire-and-forget, spawn task with owned data)
    if should_produce_secondary() {
        if let Some(secondary_producer) = secondary_producer.clone() {
            let topic_owned = topic.to_string();
            let key_owned = key.to_string();
            let payload_owned = message_str.clone();
            tokio::spawn(async move {
                let record = FutureRecord::to(&topic_owned)
                    .key(&key_owned)
                    .payload(&payload_owned);
                if let Err(err) = secondary_producer
                    .send(record, Timeout::After(Duration::from_secs(1)))
                    .await
                {
                    error!(
                        "[Kafka Secondary] Secondary Kafka produce failed: {}",
                        err.0
                    );
                }
            });
        }
    }

    Ok(())
}
