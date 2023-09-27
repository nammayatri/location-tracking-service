use actix::fut::{ready, Ready};
use actix_web::{
    body::{BoxBody, MessageBody},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
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
