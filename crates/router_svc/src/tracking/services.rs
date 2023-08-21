use super::models::{
    AuthResponseData, BulkDataReq, DriverLocation, DurationStruct, GetNearbyDriversRequest,
    LocationUpdate, Point, ResponseData, RideEndRequest, RideEndRes, RideId, RideStartRequest,
    UpdateDriverLocationRequest,
};
use crate::AppState;
use actix_web::{get, post, web, HttpRequest, HttpResponse, Responder};
use chrono::DateTime;
use chrono::Utc;
use fred::types::{GeoPosition, GeoUnit, GeoValue, RedisValue, SortOrder};
use log::info;
use rand::{thread_rng, Rng};
use rdkafka::util::Timeout;
use redis::Commands;
use std::collections::HashMap;
// use serde::{Deserialize, Serialize};
use super::super::env::{
    driver_loc_bucket_key, driver_loc_bucket_keys_with_all_vt, driver_loc_ts_key, on_ride_key,
    on_ride_loc_key,
};
use crate::types::*;
use geo::point;
use geo::Intersects;
use reqwest::header::CONTENT_TYPE;
use std::env::var;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rdkafka::producer::{FutureProducer, FutureRecord};

#[post("/ui/driver/location")]
async fn update_driver_location(
    data: web::Data<AppState>,
    param_obj: web::Json<Vec<UpdateDriverLocationRequest>>,
    req: HttpRequest,
) -> impl Responder {
    let body = param_obj.into_inner();
    let _json = serde_json::to_string(&body).unwrap();

    let start = Instant::now();
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

    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: body[0].pt.lon, y: body[0].pt.lat));
        if intersection {
            city = multi_polygon_body.region.clone();
            break;
        }
    }

    info!("city: {}", city);

    if !intersection {
        let duration_full = DurationStruct {
            dur: start.elapsed(),
        };
        let duration = serde_json::to_string(&duration_full).unwrap();
        return HttpResponse::ServiceUnavailable()
            .content_type("application/json")
            .body(duration);
    }

    let auth_url = var("AUTH_URL").expect("AUTH_URL not found");
    let bulk_loc_update_url = var("BULK_LOC_UPDATE_URL").expect("BULK_LOC_UPDATE_URL not found");

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
            let duration_full = serde_json::to_string(&DurationStruct {
                dur: start.elapsed(),
            })
            .unwrap();
            return HttpResponse::InternalServerError()
                .content_type("application/json")
                .body(duration_full);
        }

        // info!("response body: {}", response_body);

        let response = serde_json::from_str::<AuthResponseData>(&response_body).unwrap();

        // let driver_id = "4321".to_string(); //   BPP SERVICE REQUIRED HERE
        let _: () = redis_pool
            .set_with_expiry(&token, &response.driver_id, token_expiry_in_sec)
            .await
            .unwrap();

        response
    } else {
        AuthResponseData { driver_id: x }
    };

    let user_limit = data
        .sliding_window_limiter(
            &response_data.driver_id,
            data.location_update_limit,
            data.location_update_interval as u32,
            &redis_pool,
        )
        .await;
    if !user_limit.1 {
        return HttpResponse::TooManyRequests().into();
    }

    let on_ride_key = on_ride_key(&merchant_id, &city, &response_data.driver_id.clone()).await;

    let on_ride_resp = redis_pool.get_key::<String>(&on_ride_key).await.unwrap();

    println!(
        "RIDE_ID TESTING: {:?}",
        serde_json::from_str::<RideId>(&on_ride_resp)
    );
    match serde_json::from_str::<RideId>(&on_ride_resp) {
        Ok(RideId {
            on_ride: true,
            ride_id: _,
        }) => {
            for loc in body {
                info!("member: {}", loc.ts.to_rfc3339());
                let on_ride_loc_key =
                    on_ride_loc_key(&merchant_id, &city, &response_data.driver_id.clone()).await;
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

                let num = redis_pool
                    .zcard(&on_ride_loc_key)
                    .await
                    .expect("unable to zcard");
                info!("num: {}", num);

                if num == 100 {
                    let RedisValue::Array(res) = redis_pool
                        .zrange(&on_ride_loc_key, 0, -1, None, false, None, false)
                        .await
                        .unwrap() else {todo!()};

                    let mut res = res
                        .into_iter()
                        .map(|x| match x {
                            RedisValue::String(y) => {
                                String::from_utf8(y.into_inner().to_vec()).unwrap()
                            }
                            _ => String::from(""),
                        })
                        .collect::<Vec<String>>();

                    res.sort();

                    info!("res: {:?}", res);

                    let RedisValue::Array(res) = redis_pool.geopos(&on_ride_loc_key, res).await.unwrap() else {todo!()};
                    info!("New res: {:?}", res);
                    let ride_id = serde_json::from_str::<RideId>(&on_ride_resp)
                        .unwrap()
                        .ride_id;
                    let loc: Vec<Point> = res
                        .into_iter()
                        .map(|x| match x {
                            RedisValue::Array(y) => {
                                let mut y = y.into_iter();
                                let point = Point {
                                    lon: y.next().unwrap().as_f64().unwrap().into(),
                                    lat: y.next().unwrap().as_f64().unwrap().into(),
                                };
                                point
                            }
                            _ => Point { lat: 0.0, lon: 0.0 },
                        })
                        .collect::<Vec<Point>>();

                    let json = BulkDataReq {
                        ride_id: ride_id,
                        driver_id: response_data.driver_id.clone(),
                        loc: loc,
                    };

                    info!("json: {:?}", json);

                    let body = client
                        .post(&bulk_loc_update_url)
                        .header(CONTENT_TYPE, "application/json")
                        .body(serde_json::to_string(&json).unwrap())
                        .send()
                        .await
                        .unwrap();
                    let status = body.status();
                    let response_body = body.text().await.unwrap();
                    info!("response body: {}", response_body);
                    if status != 200 {
                        let duration_full = serde_json::to_string(&DurationStruct {
                            dur: start.elapsed(),
                        })
                        .unwrap();
                        return HttpResponse::Ok()
                            .content_type("application/json")
                            .body(duration_full);
                    }

                    let _: () = redis_pool.delete_key(&on_ride_loc_key).await.unwrap();
                }

                let ride_id = serde_json::from_str::<RideId>(&on_ride_resp)
                .unwrap()
                .ride_id;
                stream_updates(data.clone(), &merchant_id, &ride_id, loc).await;
            }
        }
        _ => {
            let key = driver_loc_ts_key(&response_data.driver_id.clone()).await;
            let utc_now_str = (Utc::now()).to_rfc3339();
            let _ = redis_pool.set_with_expiry(&key, utc_now_str, 90);
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
                    .push((loc.pt.lon, loc.pt.lat, response_data.driver_id.clone()));
                // println!("{:?}", entries);

                // info!("{:?}", entries);
                stream_updates(data.clone(), &merchant_id, &"".to_string(), loc).await;
            }
        }
    }

    let duration_full = serde_json::to_string(&DurationStruct {
        dur: start.elapsed(),
    })
    .unwrap();
    //logs
    // info!("Token: {:?}", token.to_str().unwrap());

    // let response_data = serde_json::to_string(&response_data).unwrap();

    // response
    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(duration_full)
    };

    response
}

async fn stream_updates(data: web::Data<AppState>, merchant_id:&String, ride_id:&String, loc:UpdateDriverLocationRequest){
    let topic = &data.driver_location_update_topic;
    let key = &data.driver_location_update_key;
    let loc = LocationUpdate {
        r_id: ride_id.to_string(),
        m_id: merchant_id.to_string(),
        ts: loc.ts,
        st : Utc::now(),
        pt: Point {
            lat: loc.pt.lat,
            lon: loc.pt.lon,
        },
        acc: loc.acc,
        ride_status: "".to_string(),
        da: true,
        mode: "".to_string(),
    };

    let message = serde_json::to_string(&loc).unwrap();

    let producer: FutureProducer = data.producer.clone();
    _ = producer
        .send(
            FutureRecord::to(topic).key(key).payload(&message),
            Timeout::After(Duration::from_secs(0)),
        )
        .await;
}

#[get("/internal/drivers/nearby")]
async fn get_nearby_drivers(
    data: web::Data<AppState>,
    param_obj: web::Json<GetNearbyDriversRequest>,
    _req: HttpRequest,
) -> impl Responder {
    let body: GetNearbyDriversRequest = param_obj.into_inner();
    let _json = serde_json::to_string(&body).unwrap();
    info!("json {:?}", _json);
    let location_expiry_in_seconds = var("LOCATION_EXPIRY")
        .expect("LOCATION_EXPIRY not found")
        .parse::<u64>()
        .unwrap();
    let current_bucket =
        Duration::as_secs(&SystemTime::elapsed(&UNIX_EPOCH).unwrap()) / location_expiry_in_seconds;
    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: body.lon, y: body.lat));
        // info!(
        //     "multipolygon contains xyz: {}", intersection
        // );
        if intersection {
            // info!("Region : {}", multi_polygon_body.region);
            city = multi_polygon_body.region.clone();
            break;
        }
    }

    if !intersection {
        return HttpResponse::ServiceUnavailable()
            .content_type("text")
            .body("No service in region");
    }
    let key = driver_loc_bucket_key(
        &body.merchant_id,
        &city,
        &body.vehicle_type,
        &current_bucket,
    )
    .await;

    let mut resp_vec: Vec<DriverLocation> = Vec::new();

    if body.vehicle_type == "" {
        let mut redis = data.redis.lock().unwrap();
        let x = driver_loc_bucket_keys_with_all_vt(&body.merchant_id, &city, &current_bucket).await;
        let all_keys = redis.keys::<_, Vec<String>>(x).unwrap();
        drop(redis);

        for key in all_keys {
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

            for item in resp {
                if let RedisValue::String(driver_id) = item.member {
                    let pos = item.position.unwrap();
                    let key = driver_loc_ts_key(&driver_id.to_string()).await;
                    let timestamp: String = redis_pool.get_key(&key).await.unwrap();
                    let timestamp = match DateTime::parse_from_rfc3339(&timestamp) {
                        Ok(x) => x.with_timezone(&Utc),
                        Err(_) => Utc::now(),
                    };
                    let driver_location = DriverLocation {
                        driver_id: driver_id.to_string(),
                        lon: pos.longitude,
                        lat: pos.latitude,
                        coordinates_calculated_at: timestamp.clone(),
                        created_at: timestamp.clone(),
                        updated_at: timestamp.clone(),
                        merchant_id: body.merchant_id.clone(),
                    };
                    resp_vec.push(driver_location);
                }
            }
        }
    } else {
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
        for item in resp {
            if let RedisValue::String(driver_id) = item.member {
                let pos = item.position.unwrap();
                let key = driver_loc_ts_key(&driver_id.to_string()).await;
                let timestamp: String = redis_pool.get_key(&key).await.unwrap();
                let timestamp = match DateTime::parse_from_rfc3339(&timestamp) {
                    Ok(x) => x.with_timezone(&Utc),
                    Err(_) => Utc::now(),
                };
                let driver_location = DriverLocation {
                    driver_id: driver_id.to_string(),
                    lon: pos.longitude,
                    lat: pos.latitude,
                    coordinates_calculated_at: timestamp.clone(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp.clone(),
                    merchant_id: body.merchant_id.clone(),
                };
                resp_vec.push(driver_location);
            }
        }
    }

    let resp = resp_vec;
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
    let _json = serde_json::to_string(&body).unwrap();

    let ride_id = path.into_inner();
    info!("rideId: {ride_id}");

    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: body.lon, y: body.lat));
        // info!(
        //     "multipolygon contains xyz: {}", intersection
        // );
        if intersection {
            // info!("Region : {}", multi_polygon_body.region);
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

    let key = on_ride_key(&body.merchant_id, &city, &body.driver_id).await;
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
    // HttpResponse::Ok().body(json)
    let response_data = ResponseData {
        result: "Success".to_string(),
    };
    info!("response_data: {:?}", response_data);
    HttpResponse::Ok()
        .content_type("application/json")
        .json(response_data)
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

    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: body.lon, y: body.lat));
        // info!(
        //     "multipolygon contains xyz: {}", intersection
        // );
        if intersection {
            // info!("Region : {}", multi_polygon_body.region);
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

    let key = on_ride_key(&body.merchant_id, &city, &body.driver_id).await;
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

    let key = on_ride_loc_key(&body.merchant_id, &city, &body.driver_id).await;

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

    let json = RideEndRes {
        ride_id: ride_id,
        driver_id: body.driver_id,
        loc: loc,
    };

    // println!("resp: {:?}", resp);
    HttpResponse::Ok()
        .content_type("application/json")
        .body(serde_json::to_string(&json).unwrap())
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
    let _json = serde_json::to_string(&body).unwrap();
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

#[get("/healthcheck")]
async fn health_check(data: web::Data<AppState>) -> impl Responder {
    let redis_pool = data.redis_pool.lock().unwrap();

    let health_check_key = format!("health_check");
    _ = redis_pool
        .set_key(&health_check_key, "driver-location-service-health-check")
        .await;
    let health_check_resp = redis_pool
        .get_key::<String>(&health_check_key)
        .await
        .unwrap();
    let nil_string = String::from("nil");

    if health_check_resp == nil_string {
        return HttpResponse::InternalServerError().content_type("application/json").json(ResponseData{result: "Service Unavailable".to_string()});
    }

    HttpResponse::Ok().content_type("application/json").json(ResponseData{result :"Service Is Up".to_string()})
}

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(update_driver_location)
        .service(get_nearby_drivers)
        .service(location)
        .service(ride_start)
        .service(ride_end)
        .service(health_check);
}
