use actix_web::web::Data;
use fred::types::RedisValue;
use geo::{point, Intersects};

use crate::{domain::types::internal::ride::*, common::{types::*, errors::AppError}};

pub async fn ride_start(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> Result<APISuccess, AppError> {
    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: request_body.lon, y: request_body.lat));
        if intersection {
            city = multi_polygon_body.region.clone();
            break;
        }
    }

    if !intersection {
        return Err(AppError::Unserviceable);
    }

    let value = RideId {
        on_ride: true,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id);
    println!("key: {}", key);

    let redis_pool = data.generic_redis.lock().await;
    let _ = redis_pool
        .set_with_expiry(&key, value, data.on_ride_expiry)
        .await;

    Ok(APISuccess::default())
}

pub async fn ride_end(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> Result<RideEndResponse, AppError> {
    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: request_body.lon, y: request_body.lat));
        if intersection {
            city = multi_polygon_body.region.clone();
            break;
        }
    }

    if !intersection {
        return Err(AppError::Unserviceable);
    }

    let value = RideId {
        on_ride: false,
        ride_id: ride_id.clone(),
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id);
    println!("key: {}", key);

    let redis_pool = data.generic_redis.lock().await;
    let _ = redis_pool
        .set_with_expiry(&key, value, data.on_ride_expiry)
        .await.unwrap();

    let key = on_ride_loc_key(&request_body.merchant_id, &city, &request_body.driver_id);

    let RedisValue::Array(res) = redis_pool
        .zrange(&key, 0, -1, None, false, None, false)
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

    println!("res: {:?}", res);

    let RedisValue::Array(res) = redis_pool.geopos(&key, res).await.unwrap() else {todo!()};
    let _: () = redis_pool.delete_key(&key).await.unwrap();


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
