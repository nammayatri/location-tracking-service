use std::fmt::Debug;
use std::str::FromStr;

use crate::call_external_api;
use crate::tools::error::AppError;
use crate::utils::{logger::*, prometheus::CALL_EXTERNAL_API};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub async fn call_api<T, U>(
    method: Method,
    url: &str,
    headers: Vec<(&str, &str)>,
    body: Option<U>,
) -> Result<T, AppError>
where
    T: DeserializeOwned,
    U: Serialize + Debug,
{
    let start_time = std::time::Instant::now();

    let client = Client::new();

    let mut header_map = HeaderMap::new();

    for (header_key, header_value) in headers {
        header_map.insert(
            HeaderName::from_str(header_key).expect("Invalid header name"),
            HeaderValue::from_str(header_value).expect("Invalid header value"),
        );
    }

    let mut request = client
        .request(method.clone(), url)
        .headers(header_map.clone());

    if let Some(body) = &body {
        let body = serde_json::to_string(body)
            .map_err(|err| AppError::SerializationError(err.to_string()))?;
        request = request.body(body);
    }

    let resp = request.send().await;

    match resp {
        Ok(resp) => {
            call_external_api!(method.as_str(), url, resp.status().as_str(), start_time);
            info!(tag = "[OUTGOING API]", request_method = %method, request_body = format!("{:?}", body), request_url = url, request_headers = format!("{:?}", header_map), response = format!("{:?}", resp), latency = format!("{:?}ms", start_time.elapsed().as_millis()));
            Ok(resp
                .json::<T>()
                .await
                .map_err(|err| AppError::DeserializationError(err.to_string()))?)
        }
        Err(err) => Err(AppError::ExternalAPICallError(err.to_string())),
    }
}
