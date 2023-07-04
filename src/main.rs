//! src/main.rs

use location_tracking_service::connection::connect;
use location_tracking_service::lists::make_rand_loc_list;
use location_tracking_service::updater::{add_to_server, push_coord};
use redis::Commands;
use std::io::{self, Read};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};
use tokio::time::Duration;

const NUM_DRIVERS: u64 = 100000; // Number of drivers whose locations need to be tracked
const TIM_WAIT: u64 = 5; // time to wait before next geoadd (in seconds)

fn main() {
    // let mut new_list: Vec<(f64, f64, String)> = Vec::new();
    // push_coord(&mut new_list, (1.0, 1.0, "loc1"));
    // println!("{:?}", new_list);

    let mut conn = connect();

    print!("Enter the number of drivers you want: ");
    let mut num = String::new();
    io::stdin().read_line(&mut num).unwrap();
    let num: u64 = num.trim().parse().unwrap();

    print!("Enter the amount of time between each batch upload: ");
    let mut tim = String::new();
    io::stdin().read_line(&mut tim).unwrap();
    let tim: u64 = tim.trim().parse().unwrap();
    //
    //
    // Sorted batched method
    //
    //

    println!("Sorted and batched method");
    let start_sorted_batch = Instant::now();
    let upd_list: Arc<Mutex<Vec<(f64, f64, String)>>> = Arc::new(Mutex::new(Vec::new()));

    let upd_list_clone = upd_list.clone();
    let t1 = thread::spawn(move || {
        // loop {
        let rand_list = make_rand_loc_list(num);
        //println!("{}", rand_list.len());
        for (lon, lat, loc_val) in rand_list {
            //println!("({}, {}, {})", item.0, item.1, item.2);
            if let Ok(mut x) = upd_list_clone.lock() {
                // x.push((lon, lat, loc_val));
                push_coord(&mut x, (lon, lat, &loc_val));
            }
        }
        // }
    });

    let upd_list_clone = upd_list.clone();
    let t2 = thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(tim));
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

    let duration_batch_sort = start_sorted_batch.elapsed();
    println!("Sorted batched approach took {}", duration_batch_sort);
}
