use crate::{
    common::{redis::*, types::*, utils::get_city},
    domain::types::internal::ride::*,
};
use actix_web::web::Data;
use fred::types::RedisValue;
use shared::tools::error::AppError;

pub async fn ride_start(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> Result<APISuccess, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    let value = RideDetails {
        ride_status: RideStatus::INPROGRESS,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let _ = data
        .generic_redis
        .lock()
        .await
        .set_with_expiry(
            &on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id),
            value,
            data.on_ride_expiry,
        )
        .await;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    let value = RideDetails {
        ride_status: RideStatus::COMPLETED,
        ride_id: ride_id.clone(),
    };
    let value = serde_json::to_string(&value).unwrap();

    let _ = data
        .generic_redis
        .lock()
        .await
        .set_with_expiry(
            &on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id),
            value,
            data.on_ride_expiry,
        )
        .await
        .unwrap();

    let RedisValue::Array(res) = data.location_redis.lock().await
        .zrange(&on_ride_loc_key(&request_body.merchant_id, &city, &request_body.driver_id), 0, -1, None, false, None, false)
        .await
        .unwrap() else {todo!()};

    let mut res = res
        .into_iter()
        .map(|x| match x {
            RedisValue::String(y) => String::from_utf8(y.into_inner().to_vec()).unwrap(),
            _ => String::from(""),
        })
        .collect::<Vec<String>>();

    res.sort();

    let RedisValue::Array(res) = data.location_redis.lock().await.geopos(&on_ride_loc_key(&request_body.merchant_id, &city, &request_body.driver_id), res).await.unwrap() else {todo!()};
    let _: () = data
        .location_redis
        .lock()
        .await
        .delete_key(&on_ride_loc_key(
            &request_body.merchant_id,
            &city,
            &request_body.driver_id,
        ))
        .await
        .unwrap();

    let mut loc: Vec<Point> = Vec::new();
    for item in res {
        let item = item.as_geo_position().unwrap().unwrap();
        loc.push(Point {
            lon: item.longitude,
            lat: item.latitude,
        });
    }

    Ok(RideEndResponse {
        ride_id: ride_id,
        driver_id: request_body.driver_id,
        loc,
    })
}

pub async fn ride_details(
    data: Data<AppState>,
    request_body: RideDetailsRequest,
) -> Result<APISuccess, AppError> {
    let city = get_city(request_body.lat, request_body.lon, data.polygon.clone())?;

    let value = RideDetails {
        ride_status: request_body.ride_status,
        ride_id: request_body.ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let result = data
        .generic_redis
        .lock()
        .await
        .set_with_expiry(
            &on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id),
            value,
            data.on_ride_expiry,
        )
        .await;
    if result.is_err() {
        return Err(AppError::InternalServerError);
    }

    Ok(APISuccess::default())
}
