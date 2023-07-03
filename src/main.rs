//! src/main.rs

use location_tracking_service::connection::connect;
use location_tracking_service::lists::make_rand_loc_list;
use redis::Commands;
// use std::{time};

use std::{
    sync::{Arc, Mutex},
    thread,
};
use tokio::time::Duration;
// use super::updater::{add_to_server, push_coord};
use location_tracking_service::hashing::*;

const NUM_DRIVERS: u64 = 10000;

const GEOSET_NAME: &str = "drivers";

pub fn add_to_server(conn: &mut redis::Connection, list: Vec<(f64, f64, String)>) {
    let mut hash_list: Vec<(String, &String)> =
        list.iter().map(|x| (to_hash(x.0, x.1), &x.2)).collect();

    hash_list.sort_by(|a, b| a.0.cmp(&b.0));

    let new_list: Vec<(f64, f64, &String)> = hash_list
        .iter()
        .map(|x| {
            let coo = from_hash(&x.0);
            (coo.x, coo.y, x.1)
        })
        .collect();

    let _: () = conn
        .geo_add(GEOSET_NAME, &new_list)
        .expect("failed to insert");
}

pub fn push_coord(
    list: &mut Vec<(f64, f64, String)>,
    item: (f64, f64, &str),
) -> Vec<(f64, f64, String)> {
    let (lon, lat, loc_val) = item;
    match list.iter().position(|(_, _, r)| r == loc_val) {
        None => list.push((lon, lat, loc_val.to_string())),
        Some(x) => list[x] = (lon, lat, loc_val.to_string()),
    }
    list.to_vec()
}

fn main() {
    // let mut new_list: Vec<(f64, f64, String)> = Vec::new();
    // push_coord(&mut new_list, (1.0, 1.0, "loc1"));
    // println!("{:?}", new_list);

    let mut conn = connect();
    let upd_list: Arc<Mutex<Vec<(f64, f64, String)>>> = Arc::new(Mutex::new(Vec::new()));

    let upd_list_clone = upd_list.clone();
    let t1 = thread::spawn(move || {
        loop {
            let rand_list = make_rand_loc_list(NUM_DRIVERS);
            //println!("{}", rand_list.len());
            for (lon, lat, loc_val) in rand_list {
                //println!("({}, {}, {})", item.0, item.1, item.2);
                if let Ok(mut x) = upd_list_clone.lock() {
                    push_coord(&mut x, (lon, lat, &loc_val));
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
                } else {
                    println!(
                        "cleaning list of size {}, and adding to redis server",
                        x.len()
                    );
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
