use actix::fut::{ready, Ready};
use actix_web::{
    body::MessageBody,
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

impl<S, B> Transform<S, ServiceRequest> for IncomingRequestMetrics
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
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

impl<S, B> Service<ServiceRequest> for IncomingRequestMetricsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let start_time = Instant::now();
        info!(tag = "[INCOMING REQUEST]", request_method = %req.method(), request_path = %req.path());

        let fut = self.service.call(req);

        Box::pin(async move {
            let response = fut.await?;
            info!(tag = "[INCOMING API]", request_method = %response.request().method(), request_path = %response.request().path(), response_status = %response.status(), latency = format!("{:?}ms", start_time.elapsed().as_millis()));
            incoming_api!(
                response.request().method().as_str(),
                response.request().uri().to_string().as_str(),
                response.status().as_str(),
                start_time
            );
            Ok(response)
        })
    }
}
