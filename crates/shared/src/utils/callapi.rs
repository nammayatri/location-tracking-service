use std::fmt::Debug;

use crate::call_external_api;
use crate::tools::error::AppError;
use crate::utils::logger::*;
use crate::utils::prometheus::CALL_EXTERNAL_API;
use reqwest::header::HeaderMap;
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::Serialize;

pub async fn call_api<T, U>(
    method: Method,
    url: &str,
    headers: HeaderMap,
    body: Option<U>,
) -> Result<T, AppError>
where
    T: DeserializeOwned,
    U: Serialize + Debug,
{
    let start_time = std::time::Instant::now();

    let client = Client::new();

    let mut request_builder = client.request(method.clone(), url).headers(headers);

    if let Some(body) = &body {
        let body = serde_json::to_string(body)
            .map_err(|err| AppError::SerializationError(err.to_string()))?;
        request_builder = request_builder.body(body);
    }

    let resp = request_builder.send().await;

    match resp {
        Ok(resp) => {
            call_external_api!(method.as_str(), url, resp.status().as_str(), start_time);
            info!("[CALL EXTERNAL API] {{ Method : {:?}, URL : {:?}, Request : {:?}, Response : {:?} }}", &method, url, &body, resp);
            Ok(resp
                .json::<T>()
                .await
                .map_err(|err| AppError::DeserializationError(err.to_string()))?)
        }
        Err(_) => Err(AppError::InternalServerError),
    }
}
