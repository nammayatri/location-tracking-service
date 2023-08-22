pub mod internal;
pub mod ui;

use actix_web::web::ServiceConfig;

pub fn handler(config: &mut ServiceConfig) {
    config
        .service(ui::location::update_driver_location)
        .service(internal::location::get_nearby_drivers)
        .service(ui::healthcheck::health_check)
        .service(internal::ride::ride_start)
        .service(internal::ride::ride_end)
        .service(internal::ride::ride_details)
        .service(internal::driver::driver_details);
}
