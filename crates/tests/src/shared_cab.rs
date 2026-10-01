/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use location_tracking_service::common::types::*;
use location_tracking_service::common::utils::get_base_vehicle_type;
use location_tracking_service::common::utils::read_dhall_config;
use location_tracking_service::environment::AppState;
use std::str::FromStr;
use tokio::sync::mpsc;

#[test]
fn shared_cab_vt_header_parses() {
    // The `vt` header goes through VehicleType::from_str (domain/api/ui/location.rs).
    assert_eq!(
        VehicleType::from_str("SHARED_CAB").ok(),
        Some(VehicleType::SharedCab)
    );
    assert_eq!(VehicleType::SharedCab.to_string(), "SHARED_CAB");
    assert_eq!(
        serde_json::from_str::<VehicleType>("\"SHARED_CAB\"").ok(),
        Some(VehicleType::SharedCab)
    );
    assert_eq!(
        get_base_vehicle_type(&VehicleType::SharedCab),
        VehicleType::SEDAN
    );
}
