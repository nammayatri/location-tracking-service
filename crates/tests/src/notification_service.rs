/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

#[tokio::test]
async fn generate_and_add_notifications() -> anyhow::Result<()> {
    let pool = shared::redis::types::RedisConnectionPool::new(
        shared::redis::types::RedisSettings::default(),
        None,
    )
    .await
    .expect("Failed to create Redis Connection Pool");

    let data = [
        ("entity.id", "181a66a5-749c-4c9f-aea5-a5418b981cf0"),
        ("entity.type", "SearchRequest"),
        ("entity.data", "{\"searchRequestValidTill\":\"2023-12-23T13:45:38.057846262Z\",\"searchRequestId\":\"181a66a5-749c-4c9f-aea5-a5418b981cf0\",\"startTime\":\"2022-08-15T13:43:30.713006Z\",\"baseFare\":100.99,\"distance\":6066,\"distanceToPickup\":316,\"fromLocation\":{\"area\":\"B-3, CA-1/99, Ganapathi Temple Rd, KHB Colony, 5th Block, Koramangala, Bengaluru, Karnataka 560095, India\",\"state\":null,\"createdAt\":\"2022-08-15T13:43:37.771311059Z\",\"country\":null,\"building\":null,\"door\":null,\"street\":null,\"lat\":12.9362698,\"city\":null,\"areaCode\":null,\"id\":\"ef9ff2e4-592b-4b00-bb07-e8d9c4965d84\",\"lon\":77.6177708,\"updatedAt\":\"2022-08-15T13:43:37.771311059Z\"},\"toLocation\":{\"area\":\"Level 8, Raheja towers, 23-24, Mahatma Gandhi Rd, Yellappa Chetty Layout, Sivanchetti Gardens, Bengaluru, Karnataka 560001, India\",\"state\":null,\"createdAt\":\"2022-08-15T13:43:37.771378308Z\",\"country\":null,\"building\":null,\"door\":null,\"street\":null,\"lat\":12.9730611,\"city\":null,\"areaCode\":null,\"id\":\"3780b236-715b-4822-b834-96bf0800c8d6\",\"lon\":77.61707299999999,\"updatedAt\":\"2022-08-15T13:43:37.771378308Z\"},\"durationToPickup\":139}"),
        ("category", "NEW_RIDE_AVAILABLE"),
        ("title", "New ride available for offering"),
        ("body", "A new ride for 15 Aug, 07:13 PM is available 316 meters away from you. Estimated base fare is 100 INR, estimated distance is 6066 meters"),
        ("show", "true"),
        ("ttl", "2023-12-21T13:45:38.057846262Z")
    ];

    for i in 1000..=1000 {
        pool.xadd(
            format!("notification:client-{}", i).as_str(),
            data.to_vec(),
            1000,
        )
        .await?;
    }

    let res = pool
        .xread(
            (1000..=1000)
                .map(|i| format!("notification:client-{}", i))
                .collect(),
            (1000..=1000)
                .map(|_| "0".to_string())
                .collect::<Vec<String>>(),
        )
        .await?;

    println!(
        "{:?}",
        notification_service::common::utils::decode_notification_payload(res)?
    );

    Ok(())
}

#[tokio::test]
async fn connect_client_without_ack() -> anyhow::Result<()> {
    use std::str::FromStr;

    let mut client = notification_service::notification_client::NotificationClient::connect(
        "http://[::1]:50051",
    )
    .await?;

    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert(
        "client-id",
        tonic::metadata::MetadataValue::from_str("notification:client-1000")?,
    );
    metadata.insert(
        "token",
        tonic::metadata::MetadataValue::from_str("token-1000")?,
    );

    let (_tx, rx) = tokio::sync::mpsc::channel(100000);

    let mut request = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(rx));
    *request.metadata_mut() = metadata;
    let response = client.stream_payload(request).await?;
    let mut inbound = response.into_inner();

    while let Some(response) = tokio_stream::StreamExt::next(&mut inbound).await {
        let notification = response?;
        println!("{:?}", notification);
    }

    Ok(())
}

#[tokio::test]
async fn connect_client_with_ack() -> anyhow::Result<()> {
    use std::str::FromStr;

    let mut client = notification_service::notification_client::NotificationClient::connect(
        "http://[::1]:50051",
    )
    .await?;

    let mut metadata = tonic::metadata::MetadataMap::new();
    metadata.insert(
        "client-id",
        tonic::metadata::MetadataValue::from_str("notification:client-1000")?,
    );
    metadata.insert(
        "token",
        tonic::metadata::MetadataValue::from_str("token-1000")?,
    );

    let (tx, rx) = tokio::sync::mpsc::channel(100000);

    let mut request = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(rx));
    *request.metadata_mut() = metadata;
    let response = client.stream_payload(request).await?;
    let mut inbound = response.into_inner();

    while let Some(response) = tokio_stream::StreamExt::next(&mut inbound).await {
        let notification = response?;
        println!("{:?}", notification);
        tx.send(notification_service::NotificationAck {
            notification_id: notification.id,
        })
        .await?;
    }

    Ok(())
}
