/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use std::time::Duration;

use crate::tools::error::AppError;
use log::{error, info};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use serde::Serialize;

/// Checks if secondary Kafka producer is enabled via environment variable.
/// Returns true if PRODUCE_SECONDARY_KAFKA is set to "true" (case-insensitive).
fn should_produce_secondary() -> bool {
    std::env::var("PRODUCE_SECONDARY_KAFKA")
        .map(|val| val.to_lowercase() == "true")
        .unwrap_or(false)
}

/// Pushes a serialized message to primary and optionally secondary Kafka topic.
///
/// This function serializes the given message into a JSON string and
/// sends it to the specified Kafka topic using the provided primary producer.
/// If secondary producer is provided and PRODUCE_SECONDARY_KAFKA env var is set to "true" (case-insensitive),
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

    // Push to primary producer
    match producer {
        Some(producer) => {
            match producer
                .send(
                    FutureRecord::to(topic).key(key).payload(&message_str),
                    Timeout::After(Duration::from_secs(1)),
                )
                .await
            {
                Ok(_) => {
                    info!("[Kafka Primary] Pushed - topic: {}, key: {}", topic, key);
                }
                Err(err) => {
                    error!(
                        "[Kafka Primary] Failed to push - topic: {}, key: {}, error: {}",
                        topic, key, err.0
                    );
                }
            }
        }
        None => {
            error!(
                "[Kafka Primary] Producer is None - topic: {}, key: {}",
                topic, key
            );
        }
    }

    // Push to secondary producer if enabled
    if should_produce_secondary() {
        match secondary_producer {
            Some(secondary_producer) => {
                match secondary_producer
                    .send(
                        FutureRecord::to(topic).key(key).payload(&message_str),
                        Timeout::After(Duration::from_secs(1)),
                    )
                    .await
                {
                    Ok(_) => {
                        info!("[Kafka Secondary] Pushed - topic: {}, key: {}", topic, key);
                    }
                    Err(err) => {
                        error!(
                            "[Kafka Secondary] Failed to push - topic: {}, key: {}, error: {}",
                            topic, key, err.0
                        );
                    }
                }
            }
            None => {
                error!(
                    "[Kafka Secondary] Producer is None - topic: {}, key: {}",
                    topic, key
                );
            }
        }
    }

    Ok(())
}
