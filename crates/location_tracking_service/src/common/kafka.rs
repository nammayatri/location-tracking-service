use std::time::Duration;

use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
};
use serde::Serialize;
use shared::utils::logger::*;

pub async fn push_to_kafka<T>(producer: &Option<FutureProducer>, topic: &str, key: &str, message: T)
where
    T: Serialize,
{
    let message = serde_json::to_string(&message);

    if let Ok(message) = message {
        match producer {
            Some(producer) => {
                _ = producer
                    .send(
                        FutureRecord::to(topic).key(key).payload(&message),
                        Timeout::After(Duration::from_secs(1)),
                    )
                    .await;
            }
            None => {
                info!("Producer is None, unable to send message");
            }
        }
    }
}
