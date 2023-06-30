//! /src/lonlat.rs

use rand::{Rng, rngs::ThreadRng};

pub fn rand_lon (rng: &mut ThreadRng) -> f64 {
  rng.gen_range(-180.0..180.0)
}

pub fn rand_lat (rng: &mut ThreadRng) -> f64 {
  rng.gen_range(-85.05..85.05)
}

// pub fn rand_loc_num (rng: &mut ThreadRng, num: u64) -> String {
//   let num = rng.gen_range(0..num);
//   format!("loc{}", num)
// }

pub fn rand_loc (rng: &mut ThreadRng, num: u64) -> (f64, f64, String) {
  (rand_lon(rng), rand_lat(rng), format!("loc{}", num))
} 