/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::environment::{CacConfig, SuperpositionClientConfig};
use actix_web::rt;
use cac_client as cac;
use shared::tools::error::AppError;
use std::time::Duration;
use superposition_client as spclient;

pub async fn init_cac_clients(cac_conf: CacConfig) -> Result<(), AppError> {
    let cac_hostname: String = cac_conf.cac_hostname;
    let polling_interval: Duration = cac_conf.cac_polling_interval;
    let update_cac_periodically = cac_conf.update_cac_periodically;
    let cac_tenants: Vec<String> = cac_conf.cac_tenants;

    for tenant in cac_tenants {
        cac::CLIENT_FACTORY
            .create_client(
                tenant.to_string(),
                update_cac_periodically,
                polling_interval,
                cac_hostname.to_string(),
            )
            .await
            .map_err(|err| {
                AppError::CacConfigFailed(format!(
                    "{}: Failed to acquire cac_client : {}",
                    tenant, err
                ))
            })?;
    }

    Ok(())
}

pub async fn init_superposition_clients(
    superposition_client_config: SuperpositionClientConfig,
    cac_conf: CacConfig,
) -> Result<(), AppError> {
    let hostname: String = superposition_client_config.superposition_hostname;
    let poll_frequency = superposition_client_config.superposition_poll_frequency;
    let cac_tenants: Vec<String> = cac_conf.cac_tenants;

    for tenant in cac_tenants {
        rt::spawn(
            spclient::CLIENT_FACTORY
                .create_client(tenant.to_string(), poll_frequency, hostname.to_string())
                .await
                .map_err(|err| {
                    AppError::CacConfigFailed(format!(
                        "{}: Failed to acquire superposition_client : {}",
                        tenant, err
                    ))
                })?
                .clone()
                .run_polling_updates(),
        );
    }

    Ok(())
}
