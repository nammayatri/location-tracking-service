//! src/hashing.rs

use geohash::{decode, encode, Coord};

pub fn to_hash(long: f64, lat: f64) -> String {
    encode(Coord { x: long, y: lat }, 11).expect("failed to encode")
}

pub fn from_hash(hash: &str) -> Coord {
    let (x, _, _) = decode(hash).expect("failed to decode");
    x
}
