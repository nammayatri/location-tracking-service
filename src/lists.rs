//! /src/lists.rs

use rand::{thread_rng, seq::SliceRandom};
use super::lonlat::rand_loc;

pub fn make_rand_loc_list (num: u64) -> Vec<(f64, f64, String)> {
  let mut i: u64 = 0;
  let mut rand_list: Vec<(f64, f64, String)> = Vec::new();
  let mut rng = rand::thread_rng();
  while i < num {
    rand_list.push(rand_loc(&mut rng, i));
    i += 1;
  }
  let mut rng = thread_rng();
  rand_list.shuffle(&mut rng);
  rand_list
}
