use std::collections::HashMap;
use std::env::var;
use std::time::Instant;

use crate::common::types::*;
use crate::domain::types::ui::location::*;
use chrono::Utc;
use fred::types::{GeoPosition, GeoValue, RedisValue};
use reqwest::header::CONTENT_TYPE;
use shared::utils::logger::*;
use actix_web::{web::Data, HttpResponse};
use geo::{point, Intersects};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseData {
    #[serde(rename = "driverId")]
    pub driver_id: String,
}

pub async fn update_driver_location(
    token: Token,
    merchant_id: MerchantId,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    request_body: Vec<UpdateDriverLocationRequest>,
) -> HttpResponse {
    let start = Instant::now();

    let token_expiry_in_sec = var("TOKEN_EXPIRY")
        .expect("TOKEN_EXPIRY not found")
        .parse::<u32>()
        .unwrap();

    let mut city = String::new();
    let mut intersection = false;
    for multi_polygon_body in &data.polygon {
        intersection = multi_polygon_body
            .multipolygon
            .intersects(&point!(x: request_body[0].pt.lon, y: request_body[0].pt.lat));
        if intersection {
            city = multi_polygon_body.region.clone();
            break;
        }
    }

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

        let response = serde_json::from_str::<AuthResponseData>(&response_body).unwrap();

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

    match serde_json::from_str::<RideId>(&on_ride_resp) {
        Ok(RideId {
            on_ride: true,
            ride_id: _,
        }) => {
            for loc in request_body {
                info!("member: {}", loc.ts.to_rfc3339());
                let on_ride_loc_key = on_ride_loc_key(&merchant_id, &city, &response_data.driver_id.clone()).await;
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

            for loc in request_body {
                entries
                    .get_mut(&merchant_id)
                    .expect("no merchant id")
                    .get_mut(&city)
                    .expect("no city")
                    .get_mut(&vehicle_type)
                    .expect("no vehicle type")
                    .push((loc.pt.lon, loc.pt.lat, response_data.driver_id.clone()));
            }
        }
    }

    let duration_full = serde_json::to_string(&DurationStruct {
        dur: start.elapsed(),
    })
    .unwrap();

    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(duration_full)
    };

    response
}