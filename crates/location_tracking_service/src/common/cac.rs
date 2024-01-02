/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::environment::{CacConfig, SuperpositionClientConfig};
use crate::tools::error::AppError;
use actix_web::rt;
use cac_client as cac;
use rand::Rng;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::time::Duration;
use superposition_client as spclient;

pub async fn init_cac_clients(cac_conf: CacConfig) -> Result<(), AppError> {
    let hostname = cac_conf.cac_hostname;
    let polling_interval: Duration = Duration::from_secs(cac_conf.cac_polling_interval);
    let update_cac_periodically = cac_conf.update_cac_periodically;
    let tenant = cac_conf.cac_tenant;

    cac::CLIENT_FACTORY
        .create_client(
            tenant.to_owned(),
            update_cac_periodically,
            polling_interval,
            hostname,
        )
        .await
        .map_err(|err| {
            AppError::CacClientInitFailed(format!(
                "{}: Failed to acquire cac_client : {}",
                tenant, err
            ))
        })?;

    Ok(())
}

pub async fn init_superposition_clients(
    superposition_client_config: SuperpositionClientConfig,
    cac_conf: CacConfig,
) -> Result<(), AppError> {
    let hostname: String = superposition_client_config.superposition_hostname;
    let poll_frequency = superposition_client_config.superposition_poll_frequency;
    let tenant = cac_conf.cac_tenant;

    rt::spawn(
        spclient::CLIENT_FACTORY
            .create_client(tenant.to_owned(), poll_frequency, hostname)
            .await
            .map_err(|err| {
                AppError::CacClientInitFailed(format!(
                    "Failed to acquire superposition_client for tenent {} : {}",
                    tenant, err
                ))
            })?
            .clone()
            .run_polling_updates(),
    );

    Ok(())
}

pub async fn get_config<T>(tenant_name: String, key: &str) -> Result<T, AppError>
where
    T: DeserializeOwned,
{
    let cac_client = cac::CLIENT_FACTORY.get_client(tenant_name.to_owned());
    let superpostion_client = spclient::CLIENT_FACTORY
        .get_client(tenant_name.to_owned())
        .await;

    match (cac_client, superpostion_client) {
        (Ok(cac_client), Ok(superpostion_client)) => {
            let mut ctx = serde_json::Map::new();
            let variant_ids = superpostion_client
                .get_applicable_variant(&json!(ctx.clone()), rand::thread_rng().gen_range(1..100))
                .await;
            ctx.insert(String::from("variantIds"), variant_ids.into());
            let res = cac_client
                .eval(ctx)
                .map_err(|err| AppError::CacConfigFailed(err.to_string()))?;

            match res.get(key) {
                Some(val) => Ok(serde_json::from_value(val.clone())
                    .map_err(|err| AppError::CacConfigFailed(err.to_string()))?),
                None => Err(AppError::CacConfigFailed(format!(
                    "Key does not exist in cac client's response for tenant {}",
                    tenant_name
                ))),
            }
        }
        _ => Err(AppError::CacConfigFailed(format!(
            "Failed to fetch instance of cac client or superposition client for tenant {}",
            tenant_name
        ))),
    }
}
