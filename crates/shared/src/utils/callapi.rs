/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

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
        let header_name = HeaderName::from_str(header_key)
            .map_err(|_| AppError::InvalidRequest(format!("Invalid Header Name : {header_key}")))?;
        let header_value = HeaderValue::from_str(header_value).map_err(|_| {
            AppError::InvalidRequest(format!("Invalid Header Value : {header_value}"))
        })?;

        header_map.insert(header_name, header_value);
    }

    let mut request = client
        .request(method.to_owned(), url)
        .headers(header_map.to_owned());

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
