//! /src/update.rs

use super::hashing::*;
use redis::Commands;

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
