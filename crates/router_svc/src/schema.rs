// @generated automatically by Diesel CLI.

pub mod atlas_driver_offer_bpp {
    diesel::table! {
        atlas_driver_offer_bpp.geometry (id) {
            #[max_length = 36]
            id -> VarChar,
            #[max_length = 255]
            region -> Varchar,
        }
    }
}
