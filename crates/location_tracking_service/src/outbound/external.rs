/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use super::types::*;
use crate::common::types::*;
use reqwest::{Method, Url};
use shared::{tools::error::AppError, utils::callapi::call_api};

pub async fn authenticate_dobpp(
    auth_url: &Url,
    token: &str,
    auth_api_key: &str,
    merchant_id: &str,
) -> Result<AuthResponseData, AppError> {
    call_api::<AuthResponseData, String>(
        Method::GET,
        auth_url,
        vec![
            ("content-type", "application/json"),
            ("token", token),
            ("api-key", auth_api_key),
            ("merchant-id", merchant_id),
        ],
        None,
    )
    .await
}

pub async fn bulk_location_update_dobpp(
    bulk_location_callback_url: &Url,
    ride_id: RideId,
    driver_id: DriverId,
    on_ride_driver_locations: Vec<Point>,
) -> Result<APISuccess, AppError> {
    call_api::<APISuccess, BulkDataReq>(
        Method::POST,
        bulk_location_callback_url,
        vec![("content-type", "application/json")],
        Some(BulkDataReq {
            ride_id,
            driver_id,
            loc: on_ride_driver_locations.clone(),
        }),
    )
    .await
}
