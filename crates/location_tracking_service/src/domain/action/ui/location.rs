use crate::common::kafka::push_to_kafka;
use crate::common::{types::*, utils::get_city};
use crate::domain::types::ui::location::*;
use crate::redis::{commands::*, keys::*};
use actix_web::web::Data;
use chrono::Utc;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use shared::tools::error::AppError;
use shared::utils::callapi::*;
use shared::utils::logger::*;
use shared::utils::prometheus;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseData {
    #[serde(rename = "driverId")]
    pub driver_id: String,
}

async fn kafka_stream_updates(
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
        acc: match loc.acc {
            Some(acc) => acc,
            None => 0,
        },
        ride_status: "".to_string(),
        da: true,
        mode: "".to_string(),
    };

    push_to_kafka(&data.producer, topic, key, loc).await;
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

    let mut driver_id = data.persistent_redis.get_key(&token).await?;

    if let None = driver_id {
        let response = call_api::<AuthResponseData, String>(
            Method::GET,
            &data.auth_url,
            vec![
                ("content-type", "application/json"),
                ("token", &token),
                ("api-key", &data.auth_api_key),
                ("merchant-id", &merchant_id),
            ],
            None,
        )
        .await?;

        let _: () = data
            .persistent_redis
            .set_with_expiry(&token, &response.driver_id, data.auth_token_expiry)
            .await
            .unwrap();

        driver_id = Some(response.driver_id);
    }

    let driver_id = driver_id.unwrap();

    let _ = data
        .sliding_window_limiter(
            &driver_id,
            data.location_update_limit,
            data.location_update_interval as u32,
            &data.persistent_redis,
        )
        .await?;

    with_lock_redis(
        data.persistent_redis.clone(),
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

    info!(
        tag = "[Location Updates]",
        "Got location updates: {driver_id} {:?}", locations
    );

    let last_location_update_ts = get_and_set_driver_last_location_update_latlon_and_timestamp(
        data.clone(),
        &driver_id,
        &merchant_id,
        &locations[locations.len() - 1],
    )
    .await
    .unwrap_or(locations[0].ts);

    match get_driver_ride_details(data.clone(), &driver_id, &merchant_id, &city).await {
        Ok(RideDetails {
            ride_id,
            ride_status: RideStatus::INPROGRESS,
        }) => {
            if locations.len() > 100 {
                error!(
                    "Way points more than 100 points {} on_ride: True",
                    locations.len()
                );
            }
            let mut geo_entries = Vec::new();
            for loc in locations {
                geo_entries.push((loc.pt.lat, loc.pt.lon, loc.ts.to_rfc3339()));
                let _ = kafka_stream_updates(data.clone(), &merchant_id, &ride_id, loc).await;
            }

            let _ = push_on_ride_driver_location(
                data.clone(),
                &driver_id,
                &merchant_id,
                &city,
                &geo_entries,
            )
            .await?;

            let on_ride_driver_location_count =
                get_on_ride_driver_location_count(data.clone(), &driver_id, &merchant_id, &city)
                    .await?;

            if on_ride_driver_location_count >= data.batch_size {
                let on_ride_driver_locations =
                    get_on_ride_driver_locations(data.clone(), &driver_id, &merchant_id, &city)
                        .await?;

                let _: APISuccess = call_api(
                    Method::POST,
                    &data.bulk_location_callback_url,
                    vec![("content-type", "application/json")],
                    Some(BulkDataReq {
                        ride_id: ride_id.clone(),
                        driver_id: driver_id.clone(),
                        loc: on_ride_driver_locations,
                    }),
                )
                .await?;
            }
        }
        _ => {
            if locations.len() > 100 {
                error!(
                    "Way points more than 100 points {} on_ride: False",
                    locations.len()
                );
            }

            let filtered_locations: Vec<UpdateDriverLocationRequest> = locations
                .clone()
                .into_iter()
                .filter(|request| {
                    let acc = match request.acc {
                        Some(acc) => acc,
                        None => 0,
                    };
                    request.ts >= last_location_update_ts
                        && acc <= data.min_location_accuracy.try_into().unwrap()
                })
                .collect();

            let mut queue = data.queue.lock().await;

            for loc in filtered_locations {
                let dimensions = Dimensions {
                    merchant_id: merchant_id.clone(),
                    city: city.clone(),
                    vehicle_type: vehicle_type.clone(),
                };

                prometheus::QUEUE_GUAGE.inc();

                queue.entry(dimensions).or_insert_with(Vec::new).push((
                    loc.pt.lat,
                    loc.pt.lon,
                    driver_id.clone(),
                ));

                let _ =
                    kafka_stream_updates(data.clone(), &merchant_id, &"".to_string(), loc).await;
            }

            drop(queue);
        }
    }

    Ok(())
}
