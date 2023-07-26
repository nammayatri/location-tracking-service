use crate::db_models::atlas_driver_offer_bpp::GeometryT;
use crate::db_utils::DbActor;

use crate::messages::GetGeometry;
use actix::Handler;
use diesel::{self, prelude::*};
use log::info;

impl Handler<GetGeometry> for DbActor {
    type Result = QueryResult<GeometryT>;

    fn handle(&mut self, msg: GetGeometry, _ctx: &mut Self::Context) -> Self::Result {
        let mut conn = self.0.get().unwrap();

        let point_str = format!("POINT({} {})", msg.lon, msg.lat);

        info!("point_str: {:?}", point_str);

        let point_query_result = diesel::sql_query(&format!("SELECT id, region FROM atlas_driver_offer_bpp.geometry WHERE ST_Contains(geom, '{}') LIMIT 1", point_str)).get_result::<GeometryT>(&mut conn);

        info!("point_query_result: {:?}", point_query_result);

        let result: GeometryT = match point_query_result {
            Ok(_geom) => _geom,
            Err(_err) => match _err {
                _ => {
                    return Err(diesel::result::Error::NotFound);
                }
            },
        };

        Ok(result)
    }
}