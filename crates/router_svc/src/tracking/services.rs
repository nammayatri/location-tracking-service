use super::models::{
    AuthResponseData, GetNearbyDriversRequest, RideEndRequest, RideStartRequest,
    UpdateDriverLocationRequest,
};
use crate::{messages::GetGeometry, AppState, DbActor};
use actix::Addr;
use actix_web::{
    get, http::header::HeaderMap, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use log::info;
use reqwest::{Client, Error, header};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, Mutex};
use geo::{Contains, polygon, LineString, Coord, coord, MultiPolygon, BooleanOps, Intersects};
use geo::{line_string, point, Polygon};
use super::bangaloreUrban;
use super::karnataka;
use super::kerala;

#[post("/ui/driver/location")]
async fn update_driver_location(
    data: web::Data<AppState>,
    param_obj: web::Json<UpdateDriverLocationRequest>,
    req: HttpRequest,
) -> impl Responder {
    let body = param_obj.into_inner();
    let json = serde_json::to_string(&body).unwrap();

    //headers
    let token = req.headers().get("token").unwrap().to_owned();
    let vechile_type = req.headers().get("vt").unwrap().to_owned();
    let merchant_id = req.headers().get("mid").unwrap().to_owned();

    // let db: Addr<DbActor> = data.as_ref().db.clone();
    // let dbData = db
    //     .send(GetGeometry {
    //         lat: body.pt.lat,
    //         lon: body.pt.lon,
    //     })
    //     .await
    //     .unwrap();

    // send error response for invalid location
    // if dbData.is_err() {   
    //     let response = {
    //         let mut response = HttpResponse::BadRequest();
    //         response.content_type("application/json");
    //         response.body("{\"error\": \"Invalid Location\"}")
    //     };
    //     return response;
    // }


    // let region = dbData.unwrap().region;
    // info!("region xyz: {:?}", region);


    let karnataka = karnataka::create_karnataka_multipolygon_body();
    // let karnatakaMultipolygon = &karnataka.multipolygon;
    // let mut allPolygons = vec![];
    // for polygon in karnatakaMultipolygon {
    //     let mut coordinates: Vec<Coord<f64>> = vec![];
    //     for point in polygon {
    //         coordinates.push(coord! { x: point.0, y: point.1 });
    //     };
        
    //     let mut polygonNew = LineString::new(coordinates);
    //     let polygon = Polygon::new(polygonNew.clone(), vec![]);
        
    //     info!("contains xyz: {}", polygon.contains(&point!(x: 77.4744 , y: 13.1819)));
    //     allPolygons.push(polygon);
    // }
    
    // let multipolygonNew : MultiPolygon = MultiPolygon::new(allPolygons);

    // info!("multipolygon karnataka contains xyz: {}", multipolygonNew.intersects(&point!(x: 76.9108, y: 12.8856)));



    let kerala = kerala::create_kerala_multipolygon_body();
    // let keralaMultipolygon = &kerala.multipolygon;

    let mut allMultiPolygons = vec![];
    allMultiPolygons.push(karnataka);
    allMultiPolygons.push(kerala);

    for multiPolygonBody in allMultiPolygons {
        let mut multiPolygon = multiPolygonBody.multipolygon;
        let mut allPolygons = vec![];
        for polygon in multiPolygon {
            let mut coordinates: Vec<Coord<f64>> = vec![];
            for point in polygon {
                coordinates.push(coord! { x: point.0, y: point.1 });
            };
            
            let mut polygonNew = LineString::new(coordinates);
            let polygon = Polygon::new(polygonNew.clone(), vec![]);
            
            // info!("contains xyz: {}", polygon.contains(&point!(x: 77.4744 , y: 13.1819)));
            allPolygons.push(polygon);
        }
        
        let multipolygonNew : MultiPolygon = MultiPolygon::new(allPolygons);

        info!("multipolygon contains xyz: {}", multipolygonNew.intersects(&point!(x: 78.94424030594956, y: 17.926496576258415)));
        if multipolygonNew.intersects(&point!(x: 78.94424030594956, y: 17.926496576258415)) {
            info!("Region : {}", multiPolygonBody.region);
        }
    }








    // let multi_polygon: MultiPolygon = MultiPolygon::new(vec![
    //     polygon![
    //       (x: 0.0, y: 0.0),
    //       (x: 2.0, y: 0.0),
    //       (x: 2.0, y: 1.0),
    //       (x: 0.0, y: 1.0),
    //     ],
    //     polygon![
    //       (x: 1.0, y: 1.0),
    //       (x: -2.0, y: 1.0),
    //       (x: -2.0, y: -1.0),
    //       (x: 1.0, y: -1.0),
    //     ]
    //   ]);


    // info!("multipolygon contains xyz: {}", multi_polygon.intersects(&point!(x: 2.0, y: 1.0)));
    


     
    // let bangaloreUrban = bangaloreUrban::create_polygon_body();
    // let polygon = bangaloreUrban.polygon;

    // let mut coordinates: Vec<Coord<f64>> = vec![];
    // for point in polygon {
    //     coordinates.push(coord! { x: point.0, y: point.1 });
    // };

    // let mut polygonNew = LineString::new(coordinates);
    // let polygon = Polygon::new(polygonNew.clone(), vec![]);

    // info!("contains xyz: {}", polygon.contains(&point!(x: 77.4744 , y: 13.1819)));
          

    // 12.9700° N, 77.6536° E
    // 12.9716° N, 77.5946° E
    // let line_string = line_string![
    // (x: 0., y: 0.),    
    // (x: 2., y: 0.),
    // (x: 2., y: 2.),
    // (x: 0., y: 2.),
    // (x: 0., y: 0.),
    // ];

    // let polygon = Polygon::new(line_string.clone(), vec![]);

    // info!("contains : {}", polygon.contains(&point!(x: 1., y: 1.)));
    

    // redis
    let mut redis_conn = data.redis_pool.lock().unwrap();
    // _ = redis_conn.set_key("key", "value".to_string()).await;

    // pushing to shared vector
    // let mut entries = data.entries.lock().unwrap();
    // entries.push((body.pt.lon, body.pt.lat, body.driverId));


    // let result = redis_conn
    //     .get_key::<String>(format!("dl:{}", token.to_str().unwrap()).as_str())
    //     .await;

    // if result.is_err() {
    //     return HttpResponse::BadRequest().body("Redis Error");
    // }

    // let result = result.unwrap();

    // let response_data: Option<AuthResponseData> = if result != "nil" {
    //     Some(AuthResponseData {
    //         driverId: result,
    //     })
    // } else {
    //     let client = reqwest::Client::new();
    //     let resp = client
    //         .get("http://127.0.0.1:8016/internal/auth")
    //         .header("token", token.clone())
    //         .header("api-key", "ae288466-2add-11ee-be56-0242ac120002")
    //         .header("merchant-id", merchant_id)
    //         .send()
    //         .await
    //         .expect("response not received");

    //     let status = resp.status();
    //     let response_body = resp.text().await.unwrap();

    //     if status != 200 {
    //         let response = {
    //             let mut response = HttpResponse::BadRequest();
    //             response.content_type("application/json");
    //             response.body(response_body)
    //         };
    //         return response;
    //     }
    //     info!("response: {}", response_body);


    //     let response_data: AuthResponseData = serde_json::from_str(&response_body).unwrap();
    //     _ = redis_conn
    //         .set_with_expiry(format!("dl:{}", token.to_str().unwrap()).as_str(), &response_data.driverId, 3600)
    //         .await;
    //     Some(response_data)
    // };

    // let res = response_data.unwrap();
    // info!("response_data xyz: {:?}", res);

    // Extract the driverId field
    // let driver_id = response_data.driverId;
    // let resToken = &response_data.token;
    // info!("data xyz: {} {}", driver_id, resToken);


    //logs
    info!("driverId: {}", token.to_str().unwrap());

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

    let redis_pool = data.redis_pool.lock();

    let response = {
        let mut response = HttpResponse::Ok();
        response.content_type("application/json");
        response.body(json)
    };
    response
    // HttpResponse::Ok().body(json)
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

    // let driverId = "";
    // let vt = "";

    // // let mut redis_conn = data.redis_pool.lock().unwrap();
    // // let result = redis_conn.set_key("key", "value".to_string()).await;

    // let mut redis_pool = data.redis_pool.lock().unwrap();
    // let result = redis_pool.set_key(format!("{}:{}",driverId,vt), "value".to_string()).await;
    // print!("{:?}", pool.get_key::<String>("chakri").await);

    let redis_pool = data.redis_pool.lock().unwrap();
    let result = redis_pool
        .set_key(
            format!("onRide:{}", body.driver_id).as_str(),
            "true".to_string(),
        )
        .await;
    if result.is_err() {
        return HttpResponse::InternalServerError().body("Error");
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

    let redis_pool = data.redis_pool.lock().unwrap();
    // let result = redis_pool
    //     .get_key::<String>(format!("pointsTracked:{}", body.driver_id).as_str())
    //     .await;
    // if result.is_err() {
    //     return HttpResponse::InternalServerError().body("Error");
    // }

    let result = redis_pool
        .set_key(
            format!("onRide:{}", body.driver_id).as_str(),
            "false".to_string(),
        )
        .await;
    if result.is_err() {
        return HttpResponse::InternalServerError().body("Error");
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

    let mut entries = data.entries.lock().unwrap();
    entries.push((body.lon, body.lat, body.driver_id));

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
        .service(location)
        .service(ride_start)
        .service(ride_end);
}
