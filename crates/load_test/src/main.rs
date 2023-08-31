//! src/main.rs
// use actix_web::{HttpRequest, HttpResponse};
// use fred::types::GeoRadiusInfo;
use chrono::{DateTime, Utc};
use rand::{thread_rng, Rng};
use reqwest::header::CONTENT_TYPE;
use serde::{Deserialize, Serialize};
use std::{
    io::{stdin, stdout, Write},
    str::FromStr,
    time::{Duration, Instant},
};
// use std::thread;
// use serde_json::Value;

const HOST_URL: &str = "http://127.0.0.1:8081";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateDriverLocationRequest {
    pub pt: Point,
    pub ts: DateTime<Utc>,
    pub acc: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuthResponseData {
    pub driver_id: String,
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GetNearbyDriversRequest {
    pub lat: f64,
    pub lon: f64,
    pub vehicle_type: String,
    pub radius: f64,
    pub merchant_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DriverLocs {
    pub lon: f64,
    pub lat: f64,
    pub driver_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RideStartRequest {
    pub lat: f64,
    pub lon: f64,
    pub driver_id: String,
    pub merchant_id: String,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RideEndRequest {
    pub lat: f64,
    pub lon: f64,
    pub driver_id: String,
    pub merchant_id: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct NearbyDriverResp {
    pub resp: Vec<(f64, f64, String)>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RideId {
    pub on_ride: bool,
    pub ride_id: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DriverRideData {
    pub resp: Vec<(f64, f64)>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DriverData {
    pub lon: f64,
    pub lat: f64,
    pub merchant_id: String,
    pub vehicle_type: String,
    pub driver_id: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DurationStruct {
    pub dur: Duration,
}

#[tokio::main]
async fn main() {
    let client = reqwest::Client::new();
    let mut rng = thread_rng();
    loop {
        println!("1. get_drivers\n2. default send driver\n3. call actual endpoint\n4. Start ride enpoint\n5. End ride endpoint\n6. Default actual endpoints\n7. On Ride testing\n0. End");
        let mut inp = String::new();
        stdin().read_line(&mut inp).unwrap();
        match inp.trim().parse::<i64>() {
            Ok(x) => {
                if x == 1 {
                    let lon = get_inp::<f64>("lon");
                    let lat = get_inp::<f64>("lat");
                    let vehicle_type = get_inp::<String>("vt");
                    let radius = get_inp::<f64>("radius");
                    let merchant_id = String::from("Namma Yatri");
                    let data = GetNearbyDriversRequest {
                        lon,
                        lat,
                        vehicle_type,
                        radius,
                        merchant_id,
                    };
                    let resp = get_drivers(data, client.clone())
                        .await
                        .json::<NearbyDriverResp>()
                        .await
                        .unwrap();
                    println!("{:?}", resp);
                } else if x == 2 {
                    let mut list: Vec<DriverData> = Vec::new();
                    let num = get_inp::<usize>("num of points");
                    for i in 0..num {
                        let lon = rng.gen_range(-180.0..180.0);
                        let lat = rng.gen_range(-85.05..85.05);
                        let driver_id = format!("loc{}", i);
                        let num = rng.gen_range(0..4);
                        let vehicle_type = match num {
                            0 => "auto".to_string(),
                            1 => "cab".to_string(),
                            2 => "suv".to_string(),
                            _ => "sedan".to_string(),
                        };
                        let merchant_id = if rng.gen_bool(0.5) {
                            "Namma Yatri".to_string()
                        } else {
                            "Uber".to_string()
                        };
                        let data = DriverData {
                            lon,
                            lat,
                            driver_id,
                            vehicle_type,
                            merchant_id,
                        };
                        list.push(data);
                        // println!("{:?}", data);

                        // thread::sleep(Duration::from_millis(10));
                    }
                    let batch_size = get_inp::<usize>("batch size");
                    let mut dur = Duration::from_secs(0);
                    if num <= batch_size {
                        println!("single");
                        let start = Instant::now();
                        dur += send_loc(list, client.clone()).await;
                        let duration = start.elapsed();
                        println!("Duration: {:?}", duration);
                    } else {
                        let num_new = num / batch_size;
                        let start = Instant::now();
                        for i in 0..num_new {
                            let start = i * batch_size;
                            let mut end = (i + 1) * batch_size;
                            end = if end > num { num } else { end };
                            dur += send_loc(list[start..end].to_vec(), client.clone()).await;
                        }
                        let duration = start.elapsed();
                        println!("Duration: {:?}", duration);
                    }
                    println!("Time Taken: {:?}", dur);
                } else if x == 3 {
                    let lon = rng.gen_range(-180.0..180.0);
                    let lat = rng.gen_range(-85.05..85.05);
                    let num = rng.gen_range(0..4);
                    let ts = Utc::now();
                    let acc = 1;
                    let vt = match num {
                        0 => "auto".to_string(),
                        1 => "cab".to_string(),
                        2 => "suv".to_string(),
                        _ => "sedan".to_string(),
                    };
                    let token = "3ab7efd4-961e-432a-835d-4d36caa042cb";
                    let m_id = "favorit0-0000-0000-0000-00000favorit".to_string();
                    let data = UpdateDriverLocationRequest {
                        pt: Point { lat, lon },
                        ts,
                        acc,
                    };
                    send_actual_loc(vec![data], client.clone(), token, m_id, vt).await;
                } else if x == 4 {
                    let lon = rng.gen_range(75.0..76.5);
                    let lat = rng.gen_range(13.0..16.5);
                    let driver_id = "favorit-suv-000000000000000000000000".to_string();
                    let merchant_id = "favorit0-0000-0000-0000-00000favorit".to_string();
                    let data = RideStartRequest {
                        lon,
                        lat,
                        driver_id,
                        merchant_id,
                    };
                    let ride_id = "1234";
                    send_start_req(data, client.clone(), ride_id).await;
                } else if x == 5 {
                    let lon = rng.gen_range(75.0..76.5);
                    let lat = rng.gen_range(13.0..16.5);
                    let driver_id = "favorit-suv-000000000000000000000000".to_string();
                    let merchant_id = "favorit0-0000-0000-0000-00000favorit".to_string();
                    let data = RideEndRequest {
                        lon,
                        lat,
                        driver_id,
                        merchant_id,
                    };
                    let ride_id = "1234";
                    let resp = send_end_req(data, client.clone(), ride_id)
                        .await
                        .json::<DriverRideData>()
                        .await
                        .unwrap();
                    println!("resp: {:?}", resp);
                } else if x == 6 {
                    let mut list: Vec<UpdateDriverLocationRequest> = Vec::new();
                    let num = get_inp::<usize>("num of points");
                    let batch_size = get_inp::<usize>("batch size");
                    for _i in 0..num {
                        let mut lon: f64;
                        let mut lat: f64;
                        let region_num = rng.gen_range(0..2);
                        for _j in 0..batch_size {
                            if region_num == 1 {
                                lon = rng.gen_range(75.0..76.5);
                                lat = rng.gen_range(13.0..16.5);
                            } else {
                                lon = rng.gen_range(76.5..77.0);
                                lat = rng.gen_range(9.5..10.0);
                            }
                            let data = UpdateDriverLocationRequest {
                                pt: Point { lon, lat },
                                acc: 1,
                                ts: Utc::now(),
                            };
                            list.push(data);
                        }
                        // println!("{:?}", data);

                        // thread::sleep(Duration::from_millis(10));
                    }

                    let merchant_id = "favorit0-0000-0000-0000-00000favorit".to_string();
                    let mut dur = Duration::from_secs(0);
                    println!("starting");
                    if num <= batch_size {
                        println!("single");
                        let num_gen = rng.gen_range(0..6);
                        let token = match num_gen {
                            0 => "favorit-admin-0000000000000000-token",
                            1 => "favorit-auto1-0000000000000000-token",
                            2 => "favorit-auto2-0000000000000000-token",
                            3 => "favorit-hatchback-000000000000-token",
                            4 => "favorit-sedan-0000000000000000-token",
                            _ => "favorit-suv-000000000000000000-token",
                        };
                        let vehicle_type = match num_gen {
                            0 => "admin".to_string(),
                            1 => "auto1".to_string(),
                            2 => "auto2".to_string(),
                            3 => "hatchback".to_string(),
                            4 => "sedan".to_string(),
                            _ => "suv".to_string(),
                        };
                        let start = Instant::now();
                        dur +=
                            send_actual_loc(list, client.clone(), token, merchant_id, vehicle_type)
                                .await;
                        let duration = start.elapsed();
                        println!("Duration: {:?}", duration);
                    } else {
                        let num_new = num / batch_size;
                        let start = Instant::now();
                        for i in 0..num_new {
                            let num_gen = rng.gen_range(0..6);
                            let token = match num_gen {
                                0 => "favorit-admin-0000000000000000-token",
                                1 => "favorit-auto1-0000000000000000-token",
                                2 => "favorit-auto2-0000000000000000-token",
                                3 => "favorit-hatchback-000000000000-token",
                                4 => "favorit-sedan-0000000000000000-token",
                                _ => "favorit-suv-000000000000000000-token",
                            };
                            let vehicle_type = match num_gen {
                                0 => "admin".to_string(),
                                1 => "auto1".to_string(),
                                2 => "auto2".to_string(),
                                3 => "hatchback".to_string(),
                                4 => "sedan".to_string(),
                                _ => "suv".to_string(),
                            };
                            let start = i * batch_size;
                            let mut end = (i + 1) * batch_size;
                            end = if end > num { num } else { end };
                            dur += send_actual_loc(
                                list[start..end].to_vec(),
                                client.clone(),
                                token,
                                merchant_id.clone(),
                                vehicle_type.clone(),
                            )
                            .await;
                        }
                        let duration = start.elapsed();
                        println!("Duration: {:?}", duration);
                    }
                    println!("Time Taken: {:?}", dur);
                } else if x == 7 {
                    let lon = rng.gen_range(75.0..76.5);
                    let lat = rng.gen_range(13.0..16.5);
                    let token = "favorit-suv-000000000000000000-token";
                    let vehicle_type = "suv".to_string();
                    let merchant_id = "favorit0-0000-0000-0000-00000favorit".to_string();
                    let data = UpdateDriverLocationRequest {
                        pt: Point { lon, lat },
                        ts: Utc::now(),
                        acc: 1,
                    };
                    let _ = send_actual_loc(
                        vec![data],
                        client.clone(),
                        token,
                        merchant_id,
                        vehicle_type,
                    )
                    .await;
                } else if x == 0 {
                    break;
                }
            }
            _ => continue,
        }
    }
}

fn get_inp<K>(query: &str) -> K
where
    K: FromStr,
{
    let mut inp = String::new();
    loop {
        print!("{}: ", query);
        stdout().flush().unwrap();
        stdin().read_line(&mut inp).unwrap();
        match inp.trim().parse::<K>() {
            Ok(x) => {
                return x;
            }
            _ => continue,
        }
    }
}

async fn send_start_req(data: RideStartRequest, client: reqwest::Client, ride_id: &str) {
    let json = serde_json::to_string(&data).unwrap();
    let _body = client
        .post(format!("{}/internal/ride/{}/start", HOST_URL, ride_id))
        .header(CONTENT_TYPE, "application/json")
        .body(json)
        .send()
        .await;
}

async fn send_end_req(
    data: RideEndRequest,
    client: reqwest::Client,
    ride_id: &str,
) -> reqwest::Response {
    let json = serde_json::to_string(&data).unwrap();
    // println!("{}", format_args!("{}/internal/ride/?rideId={}/end", HOST_URL, ride_id));
    client
        .post(format!("{}/internal/ride/{}/end", HOST_URL, ride_id))
        .header(CONTENT_TYPE, "application/json")
        .body(json)
        .send()
        .await
        .unwrap()
}

async fn send_loc(data: Vec<DriverData>, client: reqwest::Client) -> Duration {
    let json = serde_json::to_string(&data).unwrap();
    let body = client
        .post(format!("{}/location", HOST_URL))
        .header(CONTENT_TYPE, "application/json")
        .body(json.clone())
        .send()
        .await;
    // println!("{json}");
    serde_json::from_str::<DurationStruct>(&body.unwrap().text().await.unwrap())
        .unwrap()
        .dur
}

async fn send_actual_loc(
    data: Vec<UpdateDriverLocationRequest>,
    client: reqwest::Client,
    token: &str,
    m_id: String,
    vt: String,
) -> Duration {
    let json = serde_json::to_string(&data).unwrap();
    // println!("{}", json);
    let body = client
        .post(format!("{}/ui/driver/location", HOST_URL))
        .header(CONTENT_TYPE, "application/json")
        .header("token", token)
        .header("vt", vt)
        .header("mId", m_id)
        .body(json)
        .send()
        .await
        .unwrap();
    let status = body.status();
    let response_body = body.text().await.unwrap();
    if status != 200 {
        // println!("{response_body}");
        Duration::from_secs(0)
    } else {
        serde_json::from_str::<DurationStruct>(&response_body)
            .unwrap()
            .dur
    }
}

// driver-id: favorit-suv-000000000000000000000000

async fn get_drivers(data: GetNearbyDriversRequest, client: reqwest::Client) -> reqwest::Response {
    let json = serde_json::to_string(&data).unwrap();
    // println!("{}", formatted.concat());
    client
        .get(format!("{}/internal/drivers/nearby", HOST_URL))
        .header(CONTENT_TYPE, "application/json")
        .body(json)
        .send()
        .await
        .unwrap()
}
