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
