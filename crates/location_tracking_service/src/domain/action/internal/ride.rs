use std::env::var;

use actix_web::{web::Data, HttpResponse};
use fred::types::RedisValue;
use geo::{point, Intersects};
use shared::utils::logger::*;

use crate::{domain::types::internal::ride::*, common::types::*};

pub async fn ride_start(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideStartRequest,
) -> HttpResponse {
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
        return HttpResponse::ServiceUnavailable()
            .content_type("text")
            .body("No service in region");
    }

    let value = RideId {
        on_ride: true,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id).await;
    println!("key: {}", key);

    let on_ride_expiry = var("ON_RIDE_EXPIRY")
        .expect("ON_RIDE_EXPIRY not found")
        .parse::<u32>()
        .unwrap();

    if let Ok(redis_pool) = data.redis_pool.lock() {
        let result = redis_pool
            .set_with_expiry(&key, value, on_ride_expiry)
            .await;
        if result.is_err() {
            return HttpResponse::InternalServerError().body("Error");
        }
    }

    let data = DriverRideResponse { resp : Vec::new() };
    let resp = serde_json::to_string(&data).unwrap();

    let response_data = ResponseData {
        result: "Success".to_string(),
    };
    info!("response_data: {:?}", response_data);
    HttpResponse::Ok()
        .content_type("application/json")
        .json(response_data)
}

pub async fn ride_end(
    ride_id: String,
    data: Data<AppState>,
    request_body: RideEndRequest,
) -> HttpResponse {
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
        return HttpResponse::ServiceUnavailable()
            .content_type("text")
            .body("No service in region");
    }

    let value = RideId {
        on_ride: false,
        ride_id: ride_id.clone(),
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = on_ride_key(&request_body.merchant_id, &city, &request_body.driver_id).await;
    println!("key: {}", key);

    let on_ride_expiry = var("ON_RIDE_EXPIRY")
        .expect("ON_RIDE_EXPIRY not found")
        .parse::<u32>()
        .unwrap();

    let redis_pool = data.redis_pool.lock().unwrap();
    let result = redis_pool
        .set_with_expiry(&key, value, on_ride_expiry)
        .await;
    if result.is_err() {
        return HttpResponse::InternalServerError().body("Error");
    }

    let key = on_ride_loc_key(&request_body.merchant_id, &city, &request_body.driver_id).await;

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

    drop(redis_pool);

    let mut loc: Vec<Point> = Vec::new();
    for item in res {
        let item = item.as_geo_position().unwrap().unwrap();
        loc.push(Point {
            lon: item.longitude,
            lat: item.latitude,
        });
    }

    let resp = RideEndResponse {
        ride_id: ride_id,
        driver_id: request_body.driver_id,
        loc,
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&resp).unwrap())
}
