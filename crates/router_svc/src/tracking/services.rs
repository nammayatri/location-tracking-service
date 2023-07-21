use super::models::{DriverLocs, GetNearbyDriversRequest, UpdateDriverLocationRequest};
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

    // redis
    // let mut redis_conn = data.redis_pool.lock().unwrap();
    // _ = redis_conn.set_key("key", "value".to_string()).await;

    // pushing to shared vector
    // let mut entries = data.entries.lock().unwrap();
    // entries.push((body.pt.lon, body.pt.lat, body.driverId));

    //headers
    let token = req.headers().get("token").unwrap().to_owned();

    let client = reqwest::Client::new();

    //logs
    info!("Token: {}", token.to_str().unwrap());

    // response
    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(token.to_str().unwrap().to_owned())
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
    let mut current_bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / 60;
    let resp = redis_pool
        .geo_search(
            &format!("dl:loc:blr:{}:{}", body.vt, current_bucket),
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
    let resp_vec = serde_json::to_string(&resp_vec).unwrap();
    // println!("{}", resp_vec);
    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(
            [
                "{".to_string(),
                format!("\"resp\":{}", resp_vec),
                "}".to_string(),
            ]
            .concat(),
        )
    };

    response
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

    //println!("{:?}", req.headers());
    println!("headers: {:?}", req.headers());
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
        .service(location);
}
