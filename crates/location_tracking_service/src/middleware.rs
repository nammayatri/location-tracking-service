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
    web::{self, Data},
    Error,
};
use futures::future::LocalBoxFuture;
use shared::{
    incoming_api,
    tools::error::AppError,
    utils::{logger::*, prometheus::INCOMING_API},
};
use tokio::time::Instant;
use tracing::Span;
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use uuid::Uuid;

use crate::environment::AppState;

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

        let req_path = get_path(&req);
        let req_method = get_method(&req);

        let fut = self.service.call(req);
        Box::pin(async move {
            let response = fut.await?;

            calculate_metrics_from_svc_resp(
                req_path.as_str(),
                req_method.as_str(),
                &response,
                start_time,
            );

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
        let start_time = Instant::now();

        let svc = self.service.clone();

        Box::pin(async move {
            let body = req.extract::<web::Bytes>().await;

            match body {
                Ok(body) => {
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
                }
                Err(err) => {
                    if let Some(max_allowed_req_size) = req
                        .app_data::<Data<AppState>>()
                        .map(|data| data.max_allowed_req_size)
                    {
                        warn!("Size of payload is greater than the allowed limit of ({max_allowed_req_size} Bytes)");
                    }

                    let req_path = get_path(&req);
                    let req_method = get_method(&req);

                    calculate_metrics(
                        req_path.as_str(),
                        req_method.as_str(),
                        "422",
                        "UNPROCESSIBLE_REQUEST",
                        start_time,
                    );
                    Err(actix_web::Error::from(AppError::UnprocessibleRequest(
                        err.to_string(),
                    )))
                }
            }
        })
    }
}

fn bytes_to_payload(buf: web::Bytes) -> dev::Payload {
    let (_, mut pl) = h1::Payload::create(true);
    pl.unread_data(buf);
    dev::Payload::from(pl)
}

fn get_path(request: &ServiceRequest) -> String {
    let mut path = request.path().to_string();
    request
        .request()
        .match_info()
        .iter()
        .for_each(|(path_name, path_val)| {
            path = path.replace(path_val, format!(":{path_name}").as_str());
        });
    path
}

fn get_method(request: &ServiceRequest) -> String {
    request.method().to_string()
}

fn calculate_metrics_from_svc_resp(
    req_path: &str,
    req_method: &str,
    response: &ServiceResponse,
    start_time: Instant,
) {
    let resp_status = response.status();

    let resp_code = response
        .response()
        .error()
        .map(|err_resp| err_resp.to_string())
        .unwrap_or_else(|| "SUCCESS".to_string());

    calculate_metrics(
        req_path,
        req_method,
        resp_status.as_str(),
        &resp_code,
        start_time,
    );
}

fn calculate_metrics(
    path: &str,
    req_method: &str,
    resp_status: &str,
    resp_code: &str,
    start_time: Instant,
) {
    incoming_api!(req_method, path, resp_status, resp_code, start_time);
}
