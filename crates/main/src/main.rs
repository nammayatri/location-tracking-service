//! src/main.rs

use location_tracking_service::env::make_app_env;

fn main() {
    let app_env = make_app_env();

    let conn = app_env.redis_conn;
    router_svc::start_server(conn).expect("Failed to create the server");
}
