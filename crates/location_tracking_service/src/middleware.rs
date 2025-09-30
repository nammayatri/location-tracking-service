/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use std::{rc::Rc, time::Duration};

use actix::fut::{ready, Ready};
use actix_http::{h1, header::CONTENT_LENGTH};
use actix_web::{
    body::{BoxBody, MessageBody},
    dev::{self, forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    web::{self, Bytes, Data},
    Error,
};
use futures::future::LocalBoxFuture;
use tokio::time::timeout;
use tracing::warn;
use tracing::Span;
use tracing_actix_web::{DefaultRootSpanBuilder, RootSpanBuilder};
use uuid::Uuid;

use crate::{environment::AppState, tools::error::AppError};

/// Processes a service request, applying a timeout if specified in the application data.
///
/// This function applies the request timeout found in the application data, waiting for
/// the completion of the service call or the expiration of the timeout, whichever comes
/// first. If the timeout expires, a `RequestTimeout` error is returned.
pub struct RequestTimeout;

impl<S: 'static> Transform<S, ServiceRequest> for RequestTimeout
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = RequestTimeoutMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RequestTimeoutMiddleware { service }))
    }
}

pub struct RequestTimeoutMiddleware<S> {
    service: S,
}

impl<S> Service<ServiceRequest> for RequestTimeoutMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error> + 'static,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if let Some(request_timeout) = req
            .app_data::<Data<AppState>>()
            .map(|data| data.request_timeout)
        {
            let timeout_duration = Duration::from_millis(request_timeout);
            let fut = self.service.call(req);
            Box::pin(async move {
                match timeout(timeout_duration, fut).await {
                    Ok(res) => Ok(res?),
                    Err(_) => Err(actix_web::Error::from(AppError::RequestTimeout)),
                }
            })
        } else {
            let fut = self.service.call(req);
            Box::pin(fut)
        }
    }
}

/// Responsible for building and managing root spans in the domain.
///
/// `DomainRootSpanBuilder` creates root spans that encapsulate the lifecycle of a request within
/// the domain. It extracts essential information such as request_id, merchant_id, and token from
/// the headers of incoming requests to enrich the spans.
pub struct DomainRootSpanBuilder;

impl RootSpanBuilder for DomainRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let request_id = request
            .headers()
            .get("x-request-id")
            .and_then(|request_id| request_id.to_str().ok())
            .map(|str| str.to_string())
            .unwrap_or(Uuid::new_v4().to_string());

        let merchant_id = request
            .headers()
            .get("mid")
            .and_then(|merchant_id| merchant_id.to_str().ok())
            .map(|str| str.to_string());

        let token = request
            .headers()
            .get("token")
            .and_then(|token| token.to_str().ok())
            .map(|str| str.to_string());

        tracing_actix_web::root_span!(request, request_id, merchant_id, token)
    }

    fn on_request_end<B: MessageBody>(span: Span, outcome: &Result<ServiceResponse<B>, Error>) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}

/// Middleware for logging the body of incoming requests under specific conditions.
///
/// `LogIncomingRequestBody` acts as a middleware designed to capture and log the body
/// of incoming service requests if certain criteria are met. Specifically, it focuses on
/// requests that are associated with an "Unprocessible Request" error response.
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
            let log_unprocessible_req_body = req
                .app_data::<Data<AppState>>()
                .map_or(vec![], |data| data.log_unprocessible_req_body.to_owned());

            if !log_unprocessible_req_body.is_empty() {
                let body = req.extract::<web::Bytes>().await?;
                let body_clone = body.clone();

                req.set_payload(bytes_to_payload(body));

                match svc.call(req).await {
                    Ok(response) => {
                        if let Some(err_resp) = response.response().error() {
                            log_request_and_response_body(
                                log_unprocessible_req_body,
                                err_resp,
                                body_clone,
                            );
                        }
                        Ok(response)
                    }
                    Err(err_resp) => {
                        log_request_and_response_body(
                            log_unprocessible_req_body,
                            &err_resp,
                            body_clone,
                        );
                        Err(err_resp)
                    }
                }
            } else {
                Ok(svc.call(req).await?)
            }
        })
    }
}

/// Convert bytes into a payload.
///
/// Takes in web::Bytes and converts them into a developer payload which can be used
/// as an HTTP message body payload.
///
/// # Arguments
/// * `buf` - The bytes to convert into a payload.
///
/// # Returns
/// * `dev::Payload` - The resulting payload.
fn bytes_to_payload(buf: web::Bytes) -> dev::Payload {
    let (_, mut pl) = h1::Payload::create(true);
    pl.unread_data(buf);
    dev::Payload::from(pl)
}

/// Logs the request body for the allowed error codes.
///
/// # Arguments
/// * `err_codes` - Allowed error codes for whom the request body is allowed to be logged.
/// * `err_resp` - reference to an error response.
/// * `request_body` - Request body represented in byte string.
fn log_request_and_response_body(err_codes: Vec<String>, err_resp: &Error, request_body: Bytes) {
    if err_codes.contains(&err_resp.to_string()) {
        warn!("Raw Request Body: {:?}", request_body);
    }
}

/// Middleware to check the content length of incoming requests.
///
/// `CheckContentLengthMiddleware` is a middleware that examines the content length
/// of incoming service requests and compares it against a predefined limit.
/// If the content length exceeds the limit, an error response is generated.
pub struct CheckContentLength;

impl<S> Transform<S, ServiceRequest> for CheckContentLength
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = CheckContentLengthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CheckContentLengthMiddleware { service }))
    }
}

pub struct CheckContentLengthMiddleware<S> {
    service: S,
}

impl<S> Service<ServiceRequest> for CheckContentLengthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let (content_length, limit) = req
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|content_length| content_length.to_str().ok()?.parse::<usize>().ok())
            .map_or((usize::MAX, usize::MAX), |content_length| {
                req.app_data::<Data<AppState>>()
                    .map(|data| (content_length, data.max_allowed_req_size))
                    .unwrap_or((usize::MAX, usize::MAX))
            });
        let fut = self.service.call(req);
        Box::pin(async move {
            if content_length <= limit {
                Ok(fut.await?)
            } else {
                Err(actix_web::Error::from(AppError::LargePayloadSize(
                    content_length,
                    limit,
                )))
            }
        })
    }
}
