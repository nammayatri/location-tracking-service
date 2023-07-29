use super::models::{
    AuthResponseData, DriverRideData, DurationStruct, GetNearbyDriversRequest, NearbyDriverResp,
    RideEndRequest, RideId, RideStartRequest, UpdateDriverLocationRequest,
};
use crate::AppState;
use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use fred::types::{GeoPosition, GeoUnit, GeoValue, RedisValue, SortOrder};
use log::info;
use rand::{thread_rng, Rng};
use std::collections::HashMap;
// use serde::{Deserialize, Serialize};
use reqwest::{Client, Error};
use std::env::var;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use actix::Addr;

use crate::types::*;

#[post("/ui/driver/location")]
async fn update_driver_location(
    data: web::Data<AppState>,
    param_obj: web::Json<Vec<UpdateDriverLocationRequest>>,
    req: HttpRequest,
) -> impl Responder {
    let body = param_obj.into_inner();
    let _json = serde_json::to_string(&body).unwrap();
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
    let token: Token = req
        .headers()
        .get("token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let merchant_id: MerchantId = req
        .headers()
        .get("mId")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let vehicle_type: VehicleType = req
        .headers()
        .get("vt")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let token_expiry_in_sec = var("TOKEN_EXPIRY")
        .expect("TOKEN_EXPIRY not found")
        .parse::<u32>()
        .unwrap();

    let auth_url = var("AUTH_URL").expect("AUTH_URL not found");

    let client = reqwest::Client::new();
    let nil_string = String::from("nil");
    let redis_pool = data.redis_pool.lock().unwrap();

    info!("token: {}", token);
    let x = redis_pool.get_key::<Key>(&token).await.unwrap();
    let response_data = if x == nil_string {
        // println!("oh no nil");
        let resp = client
            .get(auth_url)
            .header("token", token.clone())
            .header("api-key", "ae288466-2add-11ee-be56-0242ac120002")
            .header("merchant-id", merchant_id.clone())
            .send()
            .await
            .expect("resp not received");

        let status = resp.status();
        let response_body = resp.text().await.unwrap();

        if status != 200 {
            return HttpResponse::Ok()
                .content_type("application/json")
                .body(response_body);
        }

        info!("response body: {}", response_body);

        let response = serde_json::from_str::<AuthResponseData>(&response_body).unwrap();

        // let driver_id = "4321".to_string(); //   BPP SERVICE REQUIRED HERE
        let _: () = redis_pool
            .set_with_expiry(&token, &response.driverId, token_expiry_in_sec)
            .await
            .unwrap();

        response
    } else {
        AuthResponseData { driverId: x }
    };

    let city = "blr".to_string(); // ADD REGION SYSTEM HERE

    let on_ride_key = format!(
        "ds:on_ride:{merchant_id}:{city}:{}",
        response_data.driverId
    );
    let on_ride_resp = redis_pool.get_key::<String>(&on_ride_key).await.unwrap();

    // println!("RIDE_ID TESTING: {:?}", serde_json::from_str::<RideId>(&on_ride_resp));
    match serde_json::from_str::<RideId>(&on_ride_resp) {
        Ok(RideId {
            on_ride: true,
            ride_id: _,
        }) => {
            for loc in body {
                info!("member: {}", loc.ts.to_rfc3339());
                let on_ride_loc_key = format!(
                    "dl:loc:{merchant_id}:{city}:{}",
                    response_data.driverId.clone()
                );
                let _: () = redis_pool
                    .geo_add(
                        &on_ride_loc_key,
                        GeoValue {
                            coordinates: GeoPosition {
                                longitude: loc.pt.lon,
                                latitude: loc.pt.lat,
                            },
                            member: RedisValue::String(loc.ts.to_rfc3339().try_into().unwrap()),
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
            let mut entries = data
                .entries
                .lock()
                .expect("Couldn't unwrap entries in /location");

            if !entries
                .keys()
                .map(|x| x.to_owned())
                .collect::<Vec<MerchantId>>()
                .contains(&merchant_id)
            {
                entries.insert(merchant_id.clone(), HashMap::new());
            }

            if !entries[&merchant_id]
                .keys()
                .map(|x| x.to_owned())
                .collect::<Vec<CityName>>()
                .contains(&city)
            {
                entries
                    .get_mut(&merchant_id)
                    .unwrap()
                    .insert(city.clone(), HashMap::new());
            }

            if !entries[&merchant_id][&city]
                .keys()
                .map(|x| x.to_owned())
                .collect::<Vec<VehicleType>>()
                .contains(&vehicle_type)
            {
                entries
                    .get_mut(&merchant_id)
                    .unwrap()
                    .get_mut(&city)
                    .unwrap()
                    .insert(vehicle_type.clone(), Vec::new());
            }

            for loc in body {
                entries
                    .get_mut(&merchant_id)
                    .expect("no merchant id")
                    .get_mut(&city)
                    .expect("no city")
                    .get_mut(&vehicle_type)
                    .expect("no vehicle type")
                    .push((loc.pt.lon, loc.pt.lat, response_data.driverId.clone()));
                // println!("{:?}", entries);

                // info!("{:?}", entries);
            }
        }
    }

    //logs
    // info!("Token: {:?}", token.to_str().unwrap());

    let response_data = serde_json::to_string(&response_data).unwrap();

    // response
    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(response_data)
    };

    response
}

#[get("/internal/drivers/nearby")]
async fn get_nearby_drivers(
    data: web::Data<AppState>,
    param_obj: web::Json<GetNearbyDriversRequest>,
    req: HttpRequest,
) -> impl Responder {
    let body = param_obj.into_inner();
    let _json = serde_json::to_string(&body).unwrap();
    //println!("{}",json);
    let location_expiry_in_seconds = var("LOCATION_EXPIRY")
        .expect("LOCATION_EXPIRY not found")
        .parse::<u64>()
        .unwrap();
    let current_bucket = Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / location_expiry_in_seconds;
    let city = "blr"; // BPP SERVICE REQUIRED HERE
    let key = format!(
        "dl:loc:{}:{city}:{}:{current_bucket}",
        body.merchant_id, body.vehicle_type
    );

    let redis_pool = data.redis_pool.lock().unwrap();
    let resp = redis_pool
        .geo_search(
            &key,
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
    let mut resp_vec: Vec<(f64, f64, String)> = Vec::new();
    for item in resp {
        if let RedisValue::String(driver_id) = item.member {
            let pos = item.position.unwrap();
            resp_vec.push((pos.longitude, pos.latitude, driver_id.to_string()));
        }
    }
    let resp = NearbyDriverResp { resp: resp_vec };
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
    info!("rideId: {ride_id}");

    let city = "blr"; // BPP SERVICE REQUIRED HERE

    let value = RideId {
        on_ride: true,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = format!(
        "ds:on_ride:{}:{}:{}",
        body.merchant_id, city, body.driver_id
    );
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
    let _json = serde_json::to_string(&body).unwrap();

    let ride_id = path.into_inner();

    println!("rideId: {}", ride_id);

    let city = "blr"; // BPP SERVICE REQUIRED HERE

    let value = RideId {
        on_ride: false,
        ride_id,
    };
    let value = serde_json::to_string(&value).unwrap();

    let key = format!(
        "ds:on_ride:{}:{}:{}",
        body.merchant_id, city, body.driver_id
    );
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

    let key = format!("dl:loc:{}:{}:{}", body.merchant_id, city, body.driver_id);

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

    let mut resp = Vec::new();
    for item in res {
        let item = item.as_geo_position().unwrap().unwrap();
        resp.push((item.longitude, item.latitude));
    }

    let data = DriverRideData { resp };

    let resp = serde_json::to_string(&data).unwrap();

    // println!("resp: {:?}", resp);
    HttpResponse::Ok()
        .content_type("application/json")
        .body(resp)
}

// Just trying

use crate::Location;

#[post("/location")]
async fn location(
    data: web::Data<AppState>,
    param_obj: web::Json<Vec<Location>>,
    _req: HttpRequest,
) -> impl Responder {
    let body = param_obj.into_inner();
    let json = serde_json::to_string(&body).unwrap();
    // info!("Location json: {}\n", json);
    // info!("Location body: {:?}\n", body);

    let start_full = Instant::now();

    for loc in body {
        // println!("{num}");
        let mut rng = thread_rng();

        let city = match rng.gen_range(0..3) {
            0 => "blr".to_string(),
            1 => "ccu".to_string(),
            _ => "ker".to_string(),
        };

        let mut entries = data
            .entries
            .lock()
            .expect("Couldn't unwrap entries in /location");

        if !entries
            .keys()
            .map(|x| x.to_owned())
            .collect::<Vec<MerchantId>>()
            .contains(&loc.merchant_id)
        {
            entries.insert(loc.merchant_id.clone(), HashMap::new());
        }

        if !entries[&loc.merchant_id]
            .keys()
            .map(|x| x.to_owned())
            .collect::<Vec<CityName>>()
            .contains(&city)
        {
            entries
                .get_mut(&loc.merchant_id)
                .unwrap()
                .insert(city.clone(), HashMap::new());
        }

        if !entries[&loc.merchant_id][&city]
            .keys()
            .map(|x| x.to_owned())
            .collect::<Vec<VehicleType>>()
            .contains(&loc.vehicle_type)
        {
            entries
                .get_mut(&loc.merchant_id)
                .unwrap()
                .get_mut(&city)
                .unwrap()
                .insert(loc.vehicle_type.clone(), Vec::new());
        }

        entries
            .get_mut(&loc.merchant_id)
            .expect("no merchant id")
            .get_mut(&city)
            .expect("no city")
            .get_mut(&loc.vehicle_type)
            .expect("no vehicle type")
            .push((loc.lon, loc.lat, loc.driver_id));

        // println!("{:?}", entries);
    }

    let duration_full = serde_json::to_string(&DurationStruct {
        dur: start_full.elapsed(),
    })
    .unwrap();

    // println!(
    //     "\n\n\n\n\n\n\nTime taken: {:?}\n\n\n\n\n\n\n\n\n",
    //     duration_full
    // );

    // println!("{:?}", req.headers());
    // println!("headers: {:?}", req.headers());
    // info!("Entries: {:?}", entries);

    // response
    // println!("{:?}", duration_full);
    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(duration_full)
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
