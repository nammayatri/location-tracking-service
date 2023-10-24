/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use std::rc::Rc;

use actix::fut::{ready, Ready};
use actix_http::h1;
use actix_web::{
    body::{BoxBody, MessageBody},
    dev::{self, forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    web, Error,
};
use futures::future::LocalBoxFuture;
use shared::{
    incoming_api,
    utils::{logger::*, prometheus::INCOMING_API},
};
use tokio::time::Instant;
use tracing::Span;
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use uuid::Uuid;

pub struct DomainRootSpanBuilder;

impl RootSpanBuilder for DomainRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let request_id = request.headers().get("x-request-id");
        let request_id = match request_id {
            Some(request_id) => request_id.to_str().map(|str| str.to_string()),
            None => Ok(Uuid::new_v4().to_string()),
        }
        .unwrap_or(Uuid::new_v4().to_string());
        tracing_actix_web::root_span!(request, request_id)
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}

pub struct IncomingRequestMetrics;

impl<S> Transform<S, ServiceRequest> for IncomingRequestMetrics
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = IncomingRequestMetricsMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(IncomingRequestMetricsMiddleware { service }))
    }
}

pub struct IncomingRequestMetricsMiddleware<S> {
    service: S,
}

impl<S> Service<ServiceRequest> for IncomingRequestMetricsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let start_time = Instant::now();
        info!(tag = "[INCOMING REQUEST]", request_method = %req.method(), request_path = %req.path());

        let fut = self.service.call(req);

        Box::pin(async move {
            let response = fut.await?;

            let mut path = response.request().path().to_string();
            response
                .request()
                .match_info()
                .iter()
                .for_each(|(path_name, path_val)| {
                    path = path.replace(path_val, format!(":{path_name}").as_str());
                });

            let resp_status = response.status();

            if let Some(err_resp) = response.response().error() {
                let err_resp = err_resp.to_string();
                info!(tag = "[INCOMING API]", request_method = %response.request().method(), request_path = %response.request().path(), response_status = %response.status(), resp_code = %err_resp, latency = format!("{:?}ms", start_time.elapsed().as_millis()));
                incoming_api!(
                    response.request().method().as_str(),
                    &path,
                    resp_status.as_str(),
                    err_resp.as_str(),
                    start_time
                );
            } else {
                info!(tag = "[INCOMING API]", request_method = %response.request().method(), request_path = %response.request().path(), response_status = %response.status(), resp_code = "SUCCESS", latency = format!("{:?}ms", start_time.elapsed().as_millis()));
                incoming_api!(
                    response.request().method().as_str(),
                    &path,
                    resp_status.as_str(),
                    "SUCCESS",
                    start_time
                );
            }

            Ok(response)
        })
    }
}

pub struct LogIncomingRequestBody;

impl<S: 'static> Transform<S, ServiceRequest> for LogIncomingRequestBody
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = LogIncomingRequestBodyMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(LogIncomingRequestBodyMiddleware {
            service: Rc::new(service),
        }))
    }
}

pub struct LogIncomingRequestBodyMiddleware<S> {
    service: Rc<S>,
}

impl<S> Service<ServiceRequest> for LogIncomingRequestBodyMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let svc = self.service.clone();

        Box::pin(async move {
            let body = req.extract::<web::Bytes>().await.unwrap();

            let body_clone = body.clone();

            // re-insert body back into request to be used by handlers
            req.set_payload(bytes_to_payload(body));

            let response = svc.call(req).await?;

            if let Some(err_resp) = response.response().error() {
                let err_resp = err_resp.to_string();
                if err_resp == "UNPROCESSIBLE_REQUEST" {
                    warn!("Raw Request Body: {body_clone:?}");
                }
            }

            Ok(response)
        })
    }
}

fn bytes_to_payload(buf: web::Bytes) -> dev::Payload {
    let (_, mut pl) = h1::Payload::create(true);
    pl.unread_data(buf);
    dev::Payload::from(pl)
}
