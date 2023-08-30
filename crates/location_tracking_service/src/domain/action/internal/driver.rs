/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use actix_web::web::Data;
use shared::tools::error::AppError;

use crate::{
    common::types::*, domain::types::internal::driver::*, redis::commands::set_driver_details,
};

pub async fn driver_details(
    data: Data<AppState>,
    request_body: DriverDetailsRequest,
) -> Result<APISuccess, AppError> {
    set_driver_details(data, request_body.driver_id, request_body.driver_mode).await?;

    Ok(APISuccess::default())
}
