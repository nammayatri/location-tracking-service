
// Generic Redis
pub fn on_ride_key(merchant_id: &String, city: &String, driver_id: &String) -> String {
    format!("ds:on_ride:{merchant_id}:{city}:{driver_id}")
}

// Generic Redis
pub fn driver_details_key(driver_id: &String) -> String {
    format!("ds:driver_details:{driver_id}")
}

// Generic Redis
pub fn driver_loc_ts_key(driver_id: &String) -> String {
    format!("dl:ts:{}", driver_id)
}

pub fn health_check_key() -> String {
    format!("health_check")
}

// Location Redis
pub fn on_ride_loc_key(merchant_id: &String, city: &String, driver_id: &String) -> String {
    format!("dl:loc:{merchant_id}:{city}:{driver_id}")
}

// Location Redis
pub fn driver_loc_bucket_key(
    merchant_id: &String,
    city: &String,
    vehicle_type: &String,
    bucket: &u64,
) -> String {
    format!("dl:loc:{merchant_id}:{city}:{vehicle_type}:{bucket}")
}