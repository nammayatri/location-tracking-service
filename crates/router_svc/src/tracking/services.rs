use super::models::{
    DriverLocs, GetNearbyDriversRequest, Resp, RideEndRequest, RideId, RideStartRequest,
    UpdateDriverLocationRequest,
};
use crate::AppState;
use actix_web::{
    get, http::header::HeaderMap, post, rt::System, web, App, HttpRequest, HttpResponse,
    HttpServer, Responder,
};
use fred::{
    interfaces::{GeoInterface, HashesInterface, KeysInterface, SortedSetsInterface},
    types::{
        Expiration, FromRedis, GeoPosition, GeoRadiusInfo, GeoUnit, GeoValue, MultipleGeoValues,
        RedisMap, RedisValue, RespVersion, SetOptions, SortOrder,
    },
};
use log::info;
use serde::{Deserialize, Serialize};
use std::env::var;
use std::{
    sync::{Arc, Mutex},
    thread::current,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

const BPP_URL: &str = "localhost:8016";

#[post("/ui/driver/location")]
async fn update_driver_location(
    data: web::Data<AppState>,
    param_obj: web::Json<UpdateDriverLocationRequest>,
    req: HttpRequest,
) -> impl Responder {
    let body = param_obj.into_inner();
    let json = serde_json::to_string(&body).unwrap();
    // info!("json {:?}", json);
    // info!("body {:?}", body);
    // redis
    // let mut redis_conn = data.redis_pool.lock().unwrap();
    // _ = redis_conn.set_key("key", "value".to_string()).await;

    // pushing to shared vector
    // let mut entries = data.entries.lock().unwrap();
    // entries.push((body.pt.lon, body.pt.lat, body.driverId));

    //headers
    // println!("headers: {:?}", req.headers());
    let token = req.headers().get("token").unwrap().to_owned();
    let token = token.to_str().unwrap().to_owned();

    let token_expiry_in_sec = var("TOKEN_EXPIRY")
        .expect("TOKEN_EXPIRY not found")
        .parse::<u32>()
        .unwrap();

    let client = reqwest::Client::new();
    let driver_id: String;
    let nil_string = String::from("nil");
    let mut redis_pool = data.redis_pool.lock().unwrap();
    match redis_pool.get_key::<String>(&token).await {
        Ok(x) => {
            if x == "nil".to_string() {
                println!("oh no nil");
                driver_id = "4321".to_string(); //   BPP SERVICE REQUIRED HERE
                let _: () = redis_pool
                    .set_with_expiry(&token, &driver_id, token_expiry_in_sec)
                    .await
                    .unwrap();
            } else {
                driver_id = x;
            }
        }
        _ => {
            println!("where token");
            driver_id = "4321".to_string(); //   BPP SERVICE REQUIRED HERE
            let _: () = redis_pool
                .set_with_expiry(&token, &driver_id, 30)
                .await
                .unwrap();
        }
    }

    let city = "blr".to_string(); // BPP SERVICE REQUIRED HERE

    let on_ride_key = format!("ds:on_ride:{}:{}", city, driver_id);
    let on_ride_resp = redis_pool.get_key::<String>(&on_ride_key).await.unwrap();

    match serde_json::from_str::<RideId>(&on_ride_resp) {
        Ok(resp) => {
            if resp.on_ride {
                info!("member: {}", body.ts.to_rfc3339());
                let on_ride_loc_key = format!("dl:loc:{}:{}", city, driver_id);
                let _: () = redis_pool
                    .geo_add(
                        &on_ride_loc_key,
                        GeoValue {
                            coordinates: GeoPosition {
                                longitude: body.pt.lon,
                                latitude: body.pt.lat,
                            },
                            member: RedisValue::String(
                                format!("{}", body.ts.to_rfc3339()).try_into().unwrap(),
                            ),
                        },
                        None,
                        false,
                    )
                    .await
                    .unwrap();
            }
        }
        _ => {
            drop(redis_pool);

            let mut entries = data.entries[&body.vt].lock().unwrap();
            entries.push((body.pt.lon, body.pt.lat, driver_id, city));
            // info!("{:?}", entries);
        }
    }

    //logs
    // info!("Token: {:?}", token.to_str().unwrap());

    // response
    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("text");
        response.body(token)
    };

    response
}

#[get("/internal/drivers/nearby")]
async fn get_nearby_drivers(
    data: web::Data<AppState>,
    param_obj: web::Json<GetNearbyDriversRequest>,
) -> impl Responder {
    let body = param_obj.into_inner();
    let json = serde_json::to_string(&body).unwrap();
    //println!("{}",json);
    let mut redis_pool = data.redis_pool.lock().unwrap();
    let current_bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / 60;
    let city = "blr"; // BPP SERVICE REQUIRED HERE
    let resp = redis_pool
        .geo_search(
            &format!("dl:loc:{}:{}:{}", city, body.vt, current_bucket),
            None,
            Some(GeoPosition::from((body.lon, body.lat))),
            Some((body.radius, GeoUnit::Kilometers)),
            None,
            Some(SortOrder::Asc),
            None,
            true,
            true,
            false,
        )
        .await
        .unwrap();
    let mut resp_vec: Vec<DriverLocs> = Vec::new();
    for item in resp {
        let RedisValue::String(driver_id) = item.member else {todo!()};
        let pos = item.position.unwrap();
        resp_vec.push(DriverLocs {
            lon: pos.longitude,
            lat: pos.latitude,
            driver_id: driver_id.to_string(),
        });
    }
    let resp = Resp { resp: resp_vec };
    let resp = serde_json::to_string(&resp).unwrap();
    // println!("{}", resp_vec);
    HttpResponse::Ok()
        .content_type("application/json")
        .body(resp)
}

#[post("/internal/ride/{rideId}/start")]
async fn ride_start(
    data: web::Data<AppState>,
    param_obj: web::Json<RideStartRequest>,
    path: web::Path<String>,
) -> impl Responder {
    let body = param_obj.into_inner();
    let json = serde_json::to_string(&body).unwrap();

    let ride_id = path.into_inner();
    info!("rideId: {}", ride_id);

    let city = "blr"; // BPP SERVICE REQUIRED HERE

    let value = RideId {
        on_ride: true,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = format!("ds:on_ride:{}:{}", city, body.driver_id);
    println!("key: {}", key);

    let on_ride_expiry = var("ON_RIDE_EXPIRY")
        .expect("ON_RIDE_EXPIRY not found")
        .parse::<u32>()
        .unwrap();

    if let Ok(mut redis_pool) = data.redis_pool.lock() {
        let result = redis_pool
            .set_with_expiry(&key, value, on_ride_expiry)
            .await;
        if result.is_err() {
            return HttpResponse::InternalServerError().body("Error");
        }
    }

    // log::info!("driverId: {}", body.driver_id());

    // let redis_pool = data.redis_pool.lock();
    HttpResponse::Ok().body(json)
}

#[post("/internal/ride/{rideId}/end")]
async fn ride_end(
    data: web::Data<AppState>,
    param_obj: web::Json<RideEndRequest>,
    path: web::Path<String>,
) -> impl Responder {
    let body = param_obj.into_inner();
    let json = serde_json::to_string(&body).unwrap();

    let ride_id = path.into_inner();

    info!("rideId: {}", ride_id);

    let city = "blr"; // BPP SERVICE REQUIRED HERE

    let value = RideId {
        on_ride: false,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = format!("ds:on_ride:{}:{}", city, body.driver_id);
    println!("key: {}", key);

    let on_ride_expiry = var("ON_RIDE_EXPIRY")
        .expect("ON_RIDE_EXPIRY not found")
        .parse::<u32>()
        .unwrap();

    if let Ok(mut redis_pool) = data.redis_pool.lock() {
        let result = redis_pool
            .set_with_expiry(&key, value, on_ride_expiry)
            .await;
        if result.is_err() {
            return HttpResponse::InternalServerError().body("Error");
        }
    }

    HttpResponse::Ok().body(json)
}

// Just trying

use crate::Location;

#[post("/location")]
async fn location(
    data: web::Data<AppState>,
    param_obj: web::Json<Location>,
    req: HttpRequest,
) -> impl Responder {
    let body = param_obj.into_inner();
    let json = serde_json::to_string(&body).unwrap();
    // info!("Location json: {}", json);
    // info!("Location body: {:?}", body);

    let mut entries = data.entries[&body.vt].lock().unwrap();
    entries.push((body.lon, body.lat, body.driver_id, "blr".to_string()));

    // println!("{:?}", req.headers());
    // println!("headers: {:?}", req.headers());
    // info!("Entries: {:?}", entries);

    // response
    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(json)
    };

    response
}

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(update_driver_location)
        .service(get_nearby_drivers)
        .service(location)
        .service(ride_start)
        .service(ride_end);
}
