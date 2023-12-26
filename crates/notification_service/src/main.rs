/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use actix_web::App;
use actix_web::HttpServer;
use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use futures::Stream;
use notification_service::common::types::*;
use notification_service::common::utils::diff_utc;
use notification_service::environment::AppConfig;
use notification_service::environment::AppState;
use notification_service::notification_latency;
use notification_service::notification_server::{Notification, NotificationServer};
use notification_service::reader::run_notification_reader;
use notification_service::redis::keys::notification_duration_key;
use notification_service::tools::logger::setup_tracing;
use notification_service::tools::prometheus::prometheus_metrics;
use notification_service::tools::prometheus::CONNECTED_CLIENTS;
use notification_service::tools::prometheus::NOTIFICATION_LATENCY;
use notification_service::{NotificationAck, NotificationPayload};
use shared::redis::types::RedisConnectionPool;
use std::env::var;
use std::net::Ipv4Addr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::MetadataMap;
use tonic::{transport::Server, Request, Response, Status};
use tracing::error;
use tracing::info;

pub struct NotificationService {
    read_notification_tx: Sender<(ClientId, Sender<Result<NotificationPayload, Status>>)>,
    redis_pool: Arc<RedisConnectionPool>,
}

impl NotificationService {
    pub fn new(
        read_notification_tx: Sender<(ClientId, Sender<Result<NotificationPayload, Status>>)>,
        redis_pool: Arc<RedisConnectionPool>,
    ) -> Self {
        NotificationService {
            read_notification_tx,
            redis_pool,
        }
    }
}

#[tonic::async_trait]
impl Notification for NotificationService {
    type StreamPayloadStream =
        Pin<Box<dyn Stream<Item = Result<NotificationPayload, Status>> + Send + Sync>>;

    async fn stream_payload(
        &self,
        request: Request<tonic::Streaming<NotificationAck>>,
    ) -> Result<Response<Self::StreamPayloadStream>, Status> {
        let metadata: &MetadataMap = request.metadata();
        let token = metadata.get("token").unwrap().to_str().unwrap().to_string();
        let client_id = metadata
            .get("client-id")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        CONNECTED_CLIENTS.inc();
        info!("Connection Successful - clientId : {client_id} - token : {token}");

        let (client_tx, client_rx) = mpsc::channel(100000);

        if let Err(e) = self
            .read_notification_tx
            .clone()
            .send((ClientId(client_id.to_owned()), client_tx))
            .await
        {
            println!("Failed to send to notification reader: {:?}", e);
        }

        let redis_pool = self.redis_pool.clone();
        tokio::spawn(async move {
            let mut stream = request.into_inner();

            // Acknowledgment for sent notification from the client
            while let Ok(Some(ack)) = stream.message().await {
                if let Ok(Some(start_time)) = redis_pool
                    .get_key::<DateTime<Utc>>(
                        notification_duration_key(ack.notification_id.clone()).as_str(),
                    )
                    .await
                {
                    notification_latency!(start_time);
                }
                let _ = redis_pool
                    .xdel(client_id.as_str(), ack.notification_id.as_str())
                    .await;
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(client_rx))))
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<()> {
    let dhall_config_path = var("DHALL_CONFIG")
        .unwrap_or_else(|_| "./dhall-configs/dev/notification_service.dhall".to_string());
    let app_config = serde_dhall::from_file(dhall_config_path).parse::<AppConfig>()?;

    let _guard = setup_tracing(app_config.logger_cfg);

    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .unwrap_or(&"Unknown panic");
        error!("Panic Occured : {payload}");
    }));

    let app_state = AppState::new(app_config).await;

    #[allow(clippy::type_complexity)]
    let (read_notification_tx, read_notification_rx): (
        Sender<(ClientId, Sender<Result<NotificationPayload, Status>>)>,
        Receiver<(ClientId, Sender<Result<NotificationPayload, Status>>)>,
    ) = mpsc::channel(10000);

    let read_notification_thread = run_notification_reader(
        read_notification_rx,
        app_state.redis_pool.clone(),
        app_state.reader_delay_seconds,
        app_state.retry_delay_seconds,
    );

    let prometheus = prometheus_metrics();
    let prometheus_server = HttpServer::new(move || App::new().wrap(prometheus.clone()))
        .bind((Ipv4Addr::UNSPECIFIED, app_state.prometheus_port))?
        .run();

    let addr = format!("[::1]:{}", app_state.port).parse()?;
    let notification_service =
        NotificationService::new(read_notification_tx, app_state.redis_pool.clone());
    let grpc_server = Server::builder()
        .add_service(NotificationServer::new(notification_service))
        .serve(addr);

    let (prometheus_result, grpc_result, _notification_result) =
        tokio::join!(prometheus_server, grpc_server, read_notification_thread);
    prometheus_result?;
    grpc_result?;

    Ok(())
}
