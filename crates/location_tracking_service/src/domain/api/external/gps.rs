/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use crate::domain::action::external::gps::{handle_external_gps_location, ExternalGPSLocationReq};
use crate::environment::AppState;
use actix_web::{web, HttpRequest, HttpResponse, ResponseError, Result};

#[actix_web::post("/external/gps/location")]
pub async fn external_gps_location(
    data: web::Data<AppState>,
    req: HttpRequest,
    json: web::Json<Vec<ExternalGPSLocationReq>>,
) -> Result<HttpResponse> {
    let api_key = req
        .headers()
        .get("X-API-Key")
        .and_then(|header_value| header_value.to_str().ok())
        .map(|api_key_str| api_key_str.to_string());

    let gps_batch = json.into_inner();

    match handle_external_gps_location(api_key, gps_batch, data).await {
        Ok(response) => Ok(response),
        Err(error) => Ok(error.error_response()),
    }
}
