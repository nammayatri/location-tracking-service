pub mod ui;
pub mod internal;

use actix_web::web::ServiceConfig;

pub fn handler(config: &mut ServiceConfig) {
    config
        .service(ui::location::update_driver_location)
        .service(internal::location::get_nearby_drivers);
}
