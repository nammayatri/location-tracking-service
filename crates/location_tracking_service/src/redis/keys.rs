/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
use crate::common::types::*;

// Generic Redis
pub fn on_ride_key(merchant_id: &MerchantId, city: &CityName, driver_id: &DriverId) -> String {
    format!("lts:ds:on_ride:{merchant_id}:{city}:{driver_id}")
}

// Generic Redis
pub fn driver_details_key(driver_id: &DriverId) -> String {
    format!("lts:ds:driver_details:{driver_id}")
}

// Generic Redis
pub fn driver_loc_ts_key(driver_id: &DriverId) -> String {
    format!("lts:dl:ts:{}", driver_id)
}

// Generic Redis
pub fn health_check_key() -> String {
    format!("lts:health_check")
}

// Generic Redis
pub fn driver_processing_location_update_lock_key(driver_id: &DriverId, city: &CityName) -> String {
    format!("lts:dl:processing:{driver_id}:{city}")
}

// Location Redis
pub fn on_ride_loc_key(merchant_id: &String, city: &CityName, driver_id: &DriverId) -> String {
    format!("lts:dl:loc:{merchant_id}:{city}:{driver_id}")
}

// Location Redis
pub fn driver_loc_bucket_key(
    merchant_id: &MerchantId,
    city: &CityName,
    vehicle_type: &VehicleType,
    bucket: &u64,
) -> String {
    format!("lts:dl:loc:{merchant_id}:{city}:{vehicle_type}:{bucket}")
}
