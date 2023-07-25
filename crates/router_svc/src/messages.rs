use crate::db_models::atlas_driver_offer_bpp::GeometryT;
use actix::Message;
use diesel::QueryResult;
use postgis_diesel::sql_types::Geometry;

#[derive(Message)]
#[rtype(result = "QueryResult<GeometryT>")]
pub struct GetGeometry {
    pub lon: f64,
    pub lat: f64,
}
