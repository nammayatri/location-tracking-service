use crate::tools::error::AppError;
use reqwest::header::HeaderMap;
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;

pub async fn call_api<T>(
    method: Method,
    url: &str,
    headers: HeaderMap,
    body: Option<&str>,
) -> Result<T, AppError>
where
    T: DeserializeOwned,
{
    let client = Client::new();

    let mut request_builder = client.request(method, url).headers(headers);

    if let Some(body) = body {
        request_builder = request_builder.body(body.to_string());
    }

    let resp = request_builder.send().await;

    match resp {
        Ok(value) => Ok(value.json::<T>().await.unwrap()),
        Err(_) => Err(AppError::InternalServerError),
    }
}
