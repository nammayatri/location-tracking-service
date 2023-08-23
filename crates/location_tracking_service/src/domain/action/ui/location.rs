use crate::common::{redis::*, types::*, utils::get_city};
use crate::domain::types::ui::location::*;
use actix_web::web::Data;
use chrono::{DateTime, Utc};
use fred::types::{GeoPosition, GeoValue, RedisValue};
use rdkafka::producer::FutureRecord;
use rdkafka::util::Timeout;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use shared::tools::error::AppError;
use shared::utils::callapi::*;
use shared::utils::logger::*;
use shared::utils::prometheus;
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseData {
    #[serde(rename = "driverId")]
    pub driver_id: String,
}

async fn stream_updates(
    data: Data<AppState>,
    merchant_id: &String,
    ride_id: &String,
    loc: UpdateDriverLocationRequest,
) {
    let topic = &data.driver_location_update_topic;
    let key = &data.driver_location_update_key;
    let loc = LocationUpdate {
        r_id: ride_id.to_string(),
        m_id: merchant_id.to_string(),
        ts: loc.ts,
        st: Utc::now(),
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

    match &data.producer {
        Some(producer) => {
            _ = producer
                .send(
                    FutureRecord::to(topic).key(key).payload(&message),
                    Timeout::After(Duration::from_secs(0)),
                )
                .await;
        }
        None => {
            info!("Producer is None, unable to send message");
        }
    }
}

pub async fn update_driver_location(
    token: Token,
    merchant_id: MerchantId,
    vehicle_type: VehicleType,
    data: Data<AppState>,
    mut request_body: Vec<UpdateDriverLocationRequest>,
) -> Result<APISuccess, AppError> {
    let city = get_city(
        request_body[0].pt.lat,
        request_body[0].pt.lon,
        data.polygon.clone(),
    )?;

    let mut driver_id = data.generic_redis.get_key(&token).await.unwrap();

    if driver_id == "nil".to_string() {
        let mut headers = HeaderMap::new();
        headers.insert("token", HeaderValue::from_str(&token).unwrap());
        headers.insert(
            "api-key",
            HeaderValue::from_str(data.auth_api_key.as_str()).unwrap(),
        );
        headers.insert("merchant-id", HeaderValue::from_str(&merchant_id).unwrap());

        let response = call_api::<AuthResponseData, String>(
            Method::GET,
            &data.auth_url,
            headers.clone(),
            None,
        )
        .await?;

        let _: () = data
            .generic_redis
            .set_with_expiry(&token, &response.driver_id, data.token_expiry)
            .await
            .unwrap();

        driver_id = response.driver_id;
    }

    let _ = data
        .sliding_window_limiter(
            &driver_id,
            data.location_update_limit,
            data.location_update_interval as u32,
            &data.generic_redis,
        )
        .await?;

    request_body.sort_by(|a, b| (a.ts).cmp(&b.ts));

    info!("got location updates: {driver_id} {:?}", request_body);

    let on_ride_resp = serde_json::from_str::<RideDetails>(
        &data
            .generic_redis
            .get_key::<String>(&on_ride_key(&merchant_id, &city, &driver_id))
            .await
            .unwrap(),
    )
    .unwrap();

    if on_ride_resp.ride_status == RideStatus::INPROGRESS {
        error!(
            "way points more then 100 points {0} on_ride: True",
            request_body.len()
        );
        for loc in request_body {
            let _: () = data
                .location_redis
                .geo_add(
                    &on_ride_loc_key(&merchant_id, &city, &driver_id),
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

            let num = data
                .location_redis
                .zcard(&on_ride_loc_key(&merchant_id, &city, &driver_id))
                .await
                .expect("unable to zcard");

            if num >= 100 {
                let RedisValue::Array(res) = data.location_redis
                    .zrange(&on_ride_loc_key(&merchant_id, &city, &driver_id), 0, -1, None, false, None, false)
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

                let RedisValue::Array(res) = data.location_redis.geopos(&on_ride_loc_key(&merchant_id, &city, &driver_id), res).await.unwrap() else {todo!()};
                info!("New res: {:?}", res);

                let _ = stream_updates(data.clone(), &merchant_id, &on_ride_resp.ride_id, loc);

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

                let mut headers = HeaderMap::new();
                headers.insert(
                    CONTENT_TYPE,
                    HeaderValue::from_str("application/json").unwrap(),
                );

                let _: APISuccess = call_api(
                    Method::POST,
                    &data.bulk_location_callback_url,
                    headers.clone(),
                    Some(BulkDataReq {
                        ride_id: on_ride_resp.ride_id.clone(),
                        driver_id: driver_id.clone(),
                        loc: loc,
                    }),
                )
                .await?;

                let _: () = data
                    .location_redis
                    .delete_key(&on_ride_loc_key(&merchant_id, &city, &driver_id))
                    .await
                    .unwrap();
            }
        }
    } else {
        let last_location_update_ts = data
            .generic_redis
            .get_key::<String>(&driver_loc_ts_key(&driver_id))
            .await
            .unwrap();
        let last_location_update_ts = match DateTime::parse_from_rfc3339(&last_location_update_ts) {
            Ok(x) => x.with_timezone(&Utc),
            Err(_) => Utc::now(),
        };

        let filtered_request_body: Vec<UpdateDriverLocationRequest> = request_body
            .into_iter()
            .filter(|request| {
                request.ts >= last_location_update_ts
                    && request.acc >= data.min_location_accuracy.try_into().unwrap()
            })
            .collect();

        let _ = data.generic_redis.set_with_expiry(
            &driver_loc_ts_key(&driver_id),
            (Utc::now()).to_rfc3339(), // Should be timestamp of last driver location
            data.redis_expiry.try_into().unwrap(),
        );

        let mut queue = data.queue.lock().await;

        for loc in filtered_request_body {
            let dimensions = Dimensions {
                merchant_id: merchant_id.clone(),
                city: city.clone(),
                vehicle_type: vehicle_type.clone(),
            };

            prometheus::QUEUE_GUAGE.inc();

            queue.entry(dimensions).or_insert_with(Vec::new).push((
                loc.pt.lon,
                loc.pt.lat,
                driver_id.clone(),
            ));

            let _ = stream_updates(data.clone(), &merchant_id, &"".to_string(), loc);
        }
    }

    Ok(APISuccess::default())
}
