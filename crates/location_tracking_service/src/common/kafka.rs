/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use std::time::Duration;

use crate::tools::error::AppError;
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use serde::Serialize;

/// Pushes a serialized message to a specified Kafka topic.
///
/// This function serializes the given message into a JSON string and
/// sends it to the specified Kafka topic using the provided producer.
/// It returns an `Ok(())` on successful message push, otherwise returns
/// an `AppError`.
///
/// # Parameters
/// - `producer`: An optional Kafka producer to send messages to Kafka.
/// - `topic`: The Kafka topic to which the message will be published.
/// - `key`: A string key associated with the message for Kafka.
/// - `message`: The message to be serialized and sent to Kafka.
///
/// # Returns
/// - `Ok(())`: If the message is successfully pushed to Kafka.
/// - `Err(AppError)`: If there's an error during serialization or Kafka push.
///
/// # Type Parameters
/// - `T`: The type of the message, which must implement the `Serialize` trait.
pub async fn push_to_kafka<T>(
    producer: &Option<FutureProducer>,
    topic: &str,
    key: &str,
    message: T,
) -> Result<(), AppError>
where
    T: Serialize,
{
    let message = serde_json::to_string(&message)
        .map_err(|err| AppError::SerializationError(err.to_string()))?;

    match producer {
        Some(producer) => {
            producer
                .send(
                    FutureRecord::to(topic).key(key).payload(&message),
                    Timeout::After(Duration::from_secs(1)),
                )
                .await
                .map_err(|err| AppError::KafkaPushFailed(err.0.to_string()))?;

            Ok(())
        }
        None => Err(AppError::KafkaPushFailed(
            "[Kafka] Producer is None, unable to send message".to_string(),
        )),
    }
}
