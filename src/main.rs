//! src/main.rs

use location_tracking_service::connection::connect;
use location_tracking_service::lists::make_rand_loc_list;
use redis::Commands;
// use std::{time};
use tokio::time::{interval, Duration};
use futures::future::abortable;
use std::{sync::{Arc, Mutex}, thread};
use location_tracking_service::update::add_to_server;

const NUM_DRIVERS: u64 = 10000;

#[tokio::main]
async fn main() {
    let mut conn = connect();
    let upd_list: Arc<Mutex<Vec<(f64, f64, String)>>> = Arc::new(Mutex::new(Vec::new()));
    
    let upd_list_clone = upd_list.clone();
    let t1 = thread::spawn(move || {
        loop {
            let rand_list = make_rand_loc_list(NUM_DRIVERS);
            //println!("{}", rand_list.len());
            for item in rand_list {
                //println!("({}, {}, {})", item.0, item.1, item.2);
                if let Ok(mut x) = upd_list_clone.lock() {
                    x.push(item);
                }
            }
        }
    }); 

    let upd_list_clone = upd_list.clone();
    let t2 = thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(5));
            if let Ok(mut x) = upd_list_clone.lock() {
                if x.is_empty() {
                    break;
                }
                else {
                    println!("cleaning list of size {}, and adding to redis server", x.len());
                    add_to_server(&mut conn, x.to_vec());

                // Code to see how update works when multiple values are given for same member in one update
                    // let new_list:Vec<(f64, f64, &str)> = vec![(13.02, 44.2, "loc1"), (2.01, 1.02, "loc1")];
                    // let _: () = conn.geo_add("drivers", &new_list)
                    //     .expect("failed to insert");
                    x.clear();
                }
            }
        }
    });

    t1.join().unwrap();
    t2.join().unwrap();

    println!("finished threads?");
}

