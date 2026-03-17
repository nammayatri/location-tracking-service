/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use async_trait::async_trait;
use reqwest::{Client, Method, StatusCode, Url};
use serde::Serialize;
use shared::tools::callapi::call_api_with_client;
use std::fmt::Debug;
use std::sync::LazyLock;
use std::time::Duration;

use crate::common::types::{BroadcastTraceConfig, ErssProviderConfig, TraceProvider};
use crate::tools::error::AppError;

/// Trait for external location providers that receive broadcast trace pings.
///
/// Each provider is constructed via its builder (which bakes in base_url, access_token, and
/// provider-specific config). `send_ping` only receives what varies per call: coordinates and
/// the local datetime string. Token refresh is handled through `update_token` so the provider
/// can be mutated in-place without a rebuild.
///
/// To add a new provider:
///   1. Add a variant to `TraceProvider` in common/types.rs with its own config struct.
///   2. Implement this trait for a new `XyzLocationProvider` struct with a builder.
///   3. Add one match arm in `make_provider`.
#[async_trait]
pub trait ExternalLocationProvider: Send + Sync {
    /// Send a location ping. All connection context (URL, token) is already held by the provider.
    async fn send_ping(&self, lat: f64, lon: f64, datetime_str: &str) -> Result<(), AppError>;

    /// Swap in refreshed credentials without rebuilding the provider.
    fn update_token(&mut self, access_token: String, token_expires_at: i64);

    fn access_token(&self) -> &str;
    fn token_expires_at(&self) -> i64;
}

/// Factory: build the right provider from a `BroadcastTraceConfig`. All common fields
/// (base_url, access_token, token_expires_at) are read from `cfg`; provider-specific
/// fields come from `cfg.provider`.
pub fn make_provider(
    cfg: &BroadcastTraceConfig,
) -> Result<Box<dyn ExternalLocationProvider>, AppError> {
    match &cfg.provider {
        TraceProvider::Erss(erss_cfg) => Ok(Box::new(
            ErssLocationProvider::builder()
                .base_url(cfg.base_url.clone())
                .access_token(cfg.access_token.clone())
                .token_expires_at(cfg.token_expires_at)
                .config(erss_cfg.clone())
                .build()?,
        )),
    }
}

// --------------- ERSS Provider ---------------

static ERSS_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default()
});

pub struct ErssLocationProvider {
    base_url: String,
    access_token: String,
    token_expires_at: i64,
    config: ErssProviderConfig,
    client: Client,
}

// --- Builder ---

#[derive(Default)]
pub struct ErssLocationProviderBuilder {
    base_url: Option<String>,
    access_token: Option<String>,
    token_expires_at: Option<i64>,
    config: Option<ErssProviderConfig>,
}

impl ErssLocationProvider {
    pub fn builder() -> ErssLocationProviderBuilder {
        ErssLocationProviderBuilder::default()
    }
}

impl ErssLocationProviderBuilder {
    pub fn base_url(mut self, v: impl Into<String>) -> Self {
        self.base_url = Some(v.into());
        self
    }

    pub fn access_token(mut self, v: impl Into<String>) -> Self {
        self.access_token = Some(v.into());
        self
    }

    pub fn token_expires_at(mut self, v: i64) -> Self {
        self.token_expires_at = Some(v);
        self
    }

    pub fn config(mut self, c: ErssProviderConfig) -> Self {
        self.config = Some(c);
        self
    }

    pub fn build(self) -> Result<ErssLocationProvider, AppError> {
        Ok(ErssLocationProvider {
            base_url: self.base_url.ok_or_else(|| {
                AppError::InvalidRequest("ErssLocationProvider: base_url is required".into())
            })?,
            access_token: self.access_token.ok_or_else(|| {
                AppError::InvalidRequest("ErssLocationProvider: access_token is required".into())
            })?,
            token_expires_at: self.token_expires_at.ok_or_else(|| {
                AppError::InvalidRequest(
                    "ErssLocationProvider: token_expires_at is required".into(),
                )
            })?,
            config: self.config.ok_or_else(|| {
                AppError::InvalidRequest("ErssLocationProvider: config is required".into())
            })?,
            client: ERSS_CLIENT.clone(),
        })
    }
}

// --- Trait impl ---

#[async_trait]
impl ExternalLocationProvider for ErssLocationProvider {
    async fn send_ping(&self, lat: f64, lon: f64, datetime_str: &str) -> Result<(), AppError> {
        // Packet format: dateTime,latitude,longitude,mobileNo,authId,authCode,SOS#
        let packet = format!(
            "{},{},{},{},{},{},SOS#",
            datetime_str,
            lat,
            lon,
            self.config.mobile_no,
            self.config.auth_id,
            self.config.auth_code
        );

        #[derive(Serialize, Debug)]
        struct TraceReq<'a> {
            #[serde(rename = "PACKET")]
            packet: &'a str,
        }

        let url_str = format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            self.config.trace_url_suffix
        );
        let url = Url::parse(&url_str)
            .map_err(|e| AppError::InvalidRequest(format!("Invalid trace URL: {}", e)))?;

        let bearer = format!("Bearer {}", self.access_token);
        call_api_with_client::<serde_json::Value, TraceReq, AppError>(
            &self.client,
            Method::POST,
            &url,
            vec![
                ("content-type", "application/json"),
                ("Authorization", &bearer),
            ],
            Some(TraceReq { packet: &packet }),
            None,
            Box::new(|resp| {
                Box::pin(async move {
                    if resp.status() == StatusCode::UNAUTHORIZED {
                        AppError::TraceTokenExpired
                    } else {
                        AppError::ExternalAPICallError(format!(
                            "Broadcast trace HTTP {}",
                            resp.status()
                        ))
                    }
                })
            }),
        )
        .await
        .map(|_| ())
    }

    fn update_token(&mut self, access_token: String, token_expires_at: i64) {
        self.access_token = access_token;
        self.token_expires_at = token_expires_at;
    }

    fn access_token(&self) -> &str {
        &self.access_token
    }

    fn token_expires_at(&self) -> i64 {
        self.token_expires_at
    }
}
