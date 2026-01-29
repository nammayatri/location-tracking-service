/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/
pub mod external;
pub mod internal;
pub mod ui;

use actix_web::web::ServiceConfig;

pub fn handler(config: &mut ServiceConfig) {
    config
        .service(ui::location::update_driver_location)
        .service(ui::location::track_driver_location)
        .service(internal::location::get_nearby_drivers)
        .service(ui::healthcheck::health_check)
        .service(internal::ride::ride_start)
        .service(internal::ride::ride_end)
        .service(internal::ride::ride_details)
        .service(internal::ride::get_driver_locations)
        .service(internal::location::get_drivers_location)
        .service(internal::location::driver_block_till)
        .service(internal::location::track_vehicles)
        .service(internal::location::post_track_vehicles)
        .service(external::gps::external_gps_location)
        .service(ui::location::update_rider_sos_location)
        .service(ui::location::track_rider_sos_location);
}
