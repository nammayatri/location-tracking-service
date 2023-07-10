//! src/main.rs

use location_tracking_service::connection::connect;


fn main() {
    let _conn: redis::Connection = connect();
    router_svc::start_server()
        .expect("Failed to create the server");
}
