//! src/main.rs

use location_tracking_service::connection::connect;

fn main() {
    let conn: redis::Connection = connect();
    router_svc::start_server(conn).expect("Failed to create the server");
}
