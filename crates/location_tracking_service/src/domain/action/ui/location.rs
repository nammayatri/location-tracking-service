use crate::common::{types::*, utils::get_city};
use crate::domain::types::ui::location::*;
use crate::redis::{commands::*, keys::*};
use actix_web::web::Data;
use chrono::{DateTime, Utc};
use fred::types::{GeoPosition, GeoValue};
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
                    Timeout::After(Duration::from_secs(1)),
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
    request_body: Vec<UpdateDriverLocationRequest>,
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

    with_lock_redis(
        data.generic_redis.clone(),
        driver_processing_location_update_lock_key(&merchant_id.clone(), &city.clone()).as_str(),
        60,
        process_driver_locations,
        (
            data.clone(),
            request_body.clone(),
            driver_id.clone(),
            merchant_id.clone(),
            vehicle_type.clone(),
            city.clone(),
        ),
    )
    .await?;

    Ok(APISuccess::default())
}

async fn process_driver_locations(
    args: (
        Data<AppState>,
        Vec<UpdateDriverLocationRequest>,
        DriverId,
        MerchantId,
        VehicleType,
        CityName,
    ),
) -> Result<(), AppError> {
    let (data, mut locations, driver_id, merchant_id, vehicle_type, city) = args;

    locations.sort_by(|a, b| (a.ts).cmp(&b.ts));

    info!("got location updates: {driver_id} {:?}", locations);

    let on_ride_resp = serde_json::from_str::<RideDetails>(
        &data
            .generic_redis
            .get_key::<String>(&on_ride_key(&merchant_id, &city, &driver_id))
            .await
            .unwrap(),
    )
    .unwrap();

    if on_ride_resp.ride_status == RideStatus::INPROGRESS {
        if locations.len() > 100 {
            error!(
                "way points more then 100 points {0} on_ride: True",
                locations.len()
            );
        }
        let mut multiple_geo_values = Vec::new();
        for loc in locations {
            let geo_value = GeoValue {
                coordinates: GeoPosition {
                    longitude: loc.pt.lon,
                    latitude: loc.pt.lat,
                },
                member: (loc.ts.to_rfc3339().try_into().unwrap()),
            };
            multiple_geo_values.push(geo_value);
            let _ = stream_updates(data.clone(), &merchant_id, &on_ride_resp.ride_id, loc).await;
        }

        let _ = push_driver_location(
            data.clone(),
            on_ride_loc_key(&merchant_id, &city, &driver_id),
            multiple_geo_values.into(),
        )
        .await?;

        let num = data
            .location_redis
            .zcard(&on_ride_loc_key(&merchant_id, &city, &driver_id))
            .await
            .expect("unable to zcard");

        if num >= data.batch_size {
            let mut res = zrange(
                data.clone(),
                on_ride_loc_key(&merchant_id, &city, &driver_id),
            )
            .await?;

            res.sort();

            info!("res: {:?}", res);

            // let RedisValue::Array(res) = data.location_redis.geopos(&on_ride_loc_key(&merchant_id, &city, &driver_id), res).await.unwrap() else {todo!()};
            let loc = geopos(
                data.clone(),
                on_ride_loc_key(&merchant_id, &city, &driver_id),
                res,
            )
            .await?;

            // let loc: Vec<Point> = res
            //     .into_iter()
            //     .map(|x| match x {
            //         RedisValue::Array(y) => {
            //             let mut y = y.into_iter();
            //             let point = Point {
            //                 lon: y.next().unwrap().as_f64().unwrap().into(),
            //                 lat: y.next().unwrap().as_f64().unwrap().into(),
            //             };
            //             point
            //         }
            //         _ => Point { lat: 0.0, lon: 0.0 },
            //     })
            //     .collect::<Vec<Point>>();

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
    } else {
        let last_location_update_ts = data
            .generic_redis
            .get_key::<String>(&driver_loc_ts_key(&driver_id))
            .await
            .unwrap();
        let last_location_update_ts = match DateTime::parse_from_rfc3339(&last_location_update_ts) {
            Ok(x) => x.with_timezone(&Utc),
            Err(_) => locations[0].ts,
        };

        let filtered_locations: Vec<UpdateDriverLocationRequest> = locations
            .clone()
            .into_iter()
            .filter(|request| {
                request.ts >= last_location_update_ts
                    && request.acc >= data.min_location_accuracy.try_into().unwrap()
            })
            .collect();

        let _ = data
            .generic_redis
            .set_with_expiry(
                &driver_loc_ts_key(&driver_id),
                (Utc::now()).to_rfc3339(), // Should be timestamp of last driver location
                data.redis_expiry.try_into().unwrap(),
            )
            .await
            .unwrap();

        let mut queue = data.queue.lock().await;

        for loc in filtered_locations {
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

            let _ = stream_updates(data.clone(), &merchant_id, &"".to_string(), loc).await;
        }
    }

    Ok(())
}
