use std::time::Instant;

use crate::common::{types::*, errors::*, self};
use crate::domain::types::ui::location::*;
use actix_web::web::Data;
use chrono::Utc;
use fred::types::{GeoPosition, GeoValue, RedisValue};
use geo::{point, Intersects};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use shared::utils::logger::*;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::util::Timeout;
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseData {
    #[serde(rename = "driverId")]
    pub driver_id: String,
}

async fn stream_updates(data: Data<AppState>, merchant_id:&String, ride_id:&String, loc:UpdateDriverLocationRequest){
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

pub async fn update_driver_location(
    token: Token,
    merchant_id: MerchantId,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    request_body: Vec<UpdateDriverLocationRequest>,
) -> Result<APISuccess, AppError> {
    let start = Instant::now();

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
        return Err(AppError::Unserviceable);
    }

    let client = reqwest::Client::new();
    let nil_string = String::from("nil");
    let redis_pool = data.location_redis.lock().await;

    info!("token: {}", token);
    let x = redis_pool.get_key::<Key>(&token).await.unwrap();
    info!("x: {:?}", x);
    let response_data = if x == nil_string {
        let resp = client
            .get(&data.auth_url)
            .header("token", token.clone())
            .header("api-key", "ae288466-2add-11ee-be56-0242ac120002")
            .header("merchant-id", merchant_id.clone())
            .send()
            .await
            .expect("resp not received");

        let status = resp.status();
        let response_body = resp.text().await.unwrap();

        if status != 200 {
            return Err(AppError::DriverAppAuthFailed(start.elapsed()));
        }

        let response = serde_json::from_str::<AuthResponseData>(&response_body).unwrap();

        let _: () = redis_pool
            .set_with_expiry(&token, &response.driver_id, data.token_expiry)
            .await
            .unwrap();

        response
    } else {
        AuthResponseData { driver_id: x }
    };

    let _ = data
        .sliding_window_limiter(
            &response_data.driver_id,
            data.location_update_limit,
            data.location_update_interval as u32,
            &redis_pool,
        )
        .await?;

    let on_ride_key = on_ride_key(&merchant_id, &city, &response_data.driver_id.clone());
    let on_ride_resp = redis_pool.get_key::<String>(&on_ride_key).await.unwrap();

    match serde_json::from_str::<RideDetails>(&on_ride_resp) {
        Ok(RideDetails {
            on_ride: common::types::RideStatus::INPROGRESS, 
            ride_id: _,
        }) => {
            for loc in request_body {
                info!("member: {}", loc.ts.to_rfc3339());
                let on_ride_loc_key =
                    on_ride_loc_key(&merchant_id, &city, &response_data.driver_id.clone());
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
                    let ride_id = serde_json::from_str::<RideDetails>(&on_ride_resp)
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
                        .post(&data.bulk_location_callback_url)
                        .header(CONTENT_TYPE, "application/json")
                        .body(serde_json::to_string(&json).unwrap())
                        .send()
                        .await
                        .unwrap();
                    let status = body.status();
                    let response_body = body.text().await.unwrap();
                    info!("response body: {}", response_body);
                    if status != 200 {
                        return Err(AppError::DriverBulkLocationUpdateFailed)
                    }

                    let _: () = redis_pool.delete_key(&on_ride_loc_key).await.unwrap();
                }
                let ride_id = serde_json::from_str::<RideDetails>(&on_ride_resp)
                .unwrap()
                .ride_id;
                stream_updates(data.clone(), &merchant_id, &ride_id, loc).await;
            }
        }
        _ => {
            let key = driver_loc_ts_key(&response_data.driver_id.clone());
            let utc_now_str = (Utc::now()).to_rfc3339();
            let _ = redis_pool.set_with_expiry(&key, utc_now_str, 90);

            let mut entries = data
                .entries
                .lock()
                .await;

            for loc in request_body {
                let dimensions = Dimensions {
                    merchant_id : merchant_id.clone(),
                    city : city.clone(),
                    vehicle_type : vehicle_type.clone(),
                };

                entries.entry(dimensions).or_insert_with(Vec::new).push((
                    loc.pt.lon,
                    loc.pt.lat,
                    response_data.driver_id.clone(),
                ));
                stream_updates(data.clone(), &merchant_id, &"".to_string(), loc).await;
            }
        }
    }

    Ok(APISuccess::default())
}
