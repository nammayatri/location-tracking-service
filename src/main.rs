//! src/main.rs

use location_tracking_service::connection::connect;
use location_tracking_service::lists::make_rand_loc_list;
// use location_tracking_service::updater::{add_to_server, push_coord};
use rand::Rng;
use redis::Commands;
use std::io::{self, Write};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};
use tokio::time::Duration;

const GEOSET_NAME: &str = "drivers";

// const NUM_DRIVERS: u64 = 100000; // Number of drivers whose locations need to be tracked
// const TIM_WAIT: u64 = 5; // time to wait before next geoadd (in seconds)

fn main() {
    // let mut conn = connect();

    print!("Enter the number of drivers you want: ");
    io::stdout().flush().unwrap();
    let mut num = String::new();
    io::stdin().read_line(&mut num).unwrap();
    let num: u64 = num.trim().parse().unwrap();

    print!("Enter the amount of time between each batch upload: ");
    io::stdout().flush().unwrap();
    let mut tim = String::new();
    io::stdin().read_line(&mut tim).unwrap();
    let tim: f64 = tim.trim().parse().unwrap();

    // //
    // //
    // // Sorted batched method
    // //
    // //
    // println!("{}", (tim * 1000.0).round() as u64);
    // println!("\nSorted and batched method");
    // let rand_list = make_rand_loc_list(num);

    // let upd_list: Arc<Mutex<Vec<(f64, f64, String)>>> = Arc::new(Mutex::new(Vec::new()));

    // let upd_list_clone = upd_list.clone();
    // let t1 = thread::spawn(move || {
    //     // loop {
    //     for (lon, lat, loc_val) in rand_list {
    //         if let Ok(mut x) = upd_list_clone.lock() {
    //             push_coord(&mut x, (lon, lat, &loc_val));
    //             std::mem::drop(x);
    //         }
    //     }
    //     // }
    // });

    // let mut _clone_list: Vec<(f64, f64, String)> = Vec::new();
    // let upd_list_clone = upd_list.clone();
    // let t2 = thread::spawn(move || {
    //     loop {
    //         let start_second_thread = Instant::now();
    //         thread::sleep(Duration::from_millis((tim * 1000.0).round() as u64));
    //         if let Ok(mut x) = upd_list_clone.lock() {
    //             _clone_list = x.to_vec();
    //             x.clear();
    //             std::mem::drop(x);
    //             if _clone_list.is_empty() {
    //                 break;
    //             } else {
    //                 println!(
    //                     "cleaning list of size {}, and adding to redis server",
    //                     _clone_list.len()
    //                 );

    //                 // Code to see how update works when multiple values are given for same member in one update
    //                 // let new_list:Vec<(f64, f64, &str)> = vec![(13.02, 44.2, "loc1"), (2.01, 1.02, "loc1")];
    //                 // let _: () = conn.geo_add("drivers", &new_list)
    //                 //     .expect("failed to insert");
    //                 add_to_server(&mut conn, _clone_list);
    //                 let duration_second_thread = start_second_thread.elapsed();
    //                 println!("took {:?}", duration_second_thread);
    //             }
    //         }
    //     }
    // });

    // let start_sorted_batch = Instant::now();

    // t1.join().unwrap();
    // t2.join().unwrap();

    // let duration_batch_sort = start_sorted_batch.elapsed();
    // println!("Sorted batched approach took {:?}\n", duration_batch_sort);

    //
    //
    // Unsorted batched method
    //
    //
    //

    let mut conn = connect();
    let rand_list = make_rand_loc_list(num);
    println!("\nUnsorted batched method");
    let upd_list: Arc<Mutex<Vec<(f64, f64, String)>>> = Arc::new(Mutex::new(Vec::new()));

    let upd_list_clone = upd_list.clone();
    let t1 = thread::spawn(move || {
        // loop {
        let mut rng = rand::thread_rng();
        for (lon, lat, loc_val) in rand_list {
            thread::sleep(Duration::from_micros(rng.gen_range(1..100)));
            if let Ok(mut x) = upd_list_clone.lock() {
                x.push((lon, lat, loc_val));
                std::mem::drop(x);
            }
        }
        // }
    });

    let upd_list_clone = upd_list.clone();
    let mut _clone_list: Vec<(f64, f64, String)> = Vec::new();
    let t2 = thread::spawn(move || {
        loop {
            let start_second_thread = Instant::now();
            thread::sleep(Duration::from_millis((tim * 1000.0).round() as u64));
            if let Ok(mut x) = upd_list_clone.lock() {
                _clone_list = x.to_vec();
                x.clear();
                std::mem::drop(x);
                if _clone_list.is_empty() {
                    let duration_second_thread = start_second_thread.elapsed();
                    println!("took {:?}", duration_second_thread);
                    break;
                } else {
                    println!(
                        "cleaning list of size {}, and adding to redis server",
                        _clone_list.len()
                    );

                    // Code to see how update works when multiple values are given for same member in one update
                    // let new_list:Vec<(f64, f64, &str)> = vec![(13.02, 44.2, "loc1"), (2.01, 1.02, "loc1")];
                    // let _: () = conn.geo_add("drivers", &new_list)
                    //     .expect("failed to insert");
                    let _: () = conn
                        .geo_add(GEOSET_NAME, _clone_list)
                        .expect("Failed to add to server");

                    let duration_second_thread = start_second_thread.elapsed();
                    println!("took {:?}", duration_second_thread);
                }
            }
        }
    });

    let start_unsorted_batch = Instant::now();
    t1.join().unwrap();
    t2.join().unwrap();

    let duration_batch_unsort = start_unsorted_batch.elapsed();
    println!(
        "Unsorted batched approach took {:?}\n",
        duration_batch_unsort
    );

    //
    //
    // Unbatched method
    //
    //

    let mut conn = connect();
    let rand_list = make_rand_loc_list(num);
    println!("\nUnbatched method");
    let mut rng = rand::thread_rng();
    let start_no_batch = Instant::now();

    for item in rand_list {
        thread::sleep(Duration::from_micros(rng.gen_range(1..100)));
        let _: () = conn
            .geo_add(GEOSET_NAME, item)
            .expect("Couldn't add to server");
    }

    let duration_no_batch = start_no_batch.elapsed();
    println!("No batch approach took {:?}", duration_no_batch);

    //
    //
    // Delete entries
    //
    //

    let _: () = conn
        .zremrangebyrank(GEOSET_NAME, 0, num.try_into().unwrap())
        .expect("Failed to delete");
}
