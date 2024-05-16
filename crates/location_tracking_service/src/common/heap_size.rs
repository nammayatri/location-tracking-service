/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use super::types::*;
use std::mem::size_of;

#[allow(dead_code)]
trait HeapSizeOf {
    fn heap_size_of(&self) -> usize;
}

impl HeapSizeOf for String {
    fn heap_size_of(&self) -> usize {
        self.capacity() * size_of::<char>()
    }
}

impl<T> HeapSizeOf for Vec<T>
where
    T: HeapSizeOf,
{
    fn heap_size_of(&self) -> usize {
        self.iter().map(|item| item.heap_size_of()).sum::<usize>()
            + self.capacity() * size_of::<T>()
    }
}

impl HeapSizeOf for MultiPolygonBody {
    fn heap_size_of(&self) -> usize {
        // The size of the region string on the heap.
        let region_size = self.region.capacity() * size_of::<char>();

        // For the MultiPolygon, we must manually estimate the size.
        // Assuming each polygon in the multipolygon has an exterior and potential interior polygons (holes).
        let multipolygon_size = self
            .multipolygon
            .0
            .iter()
            .map(|polygon| {
                // Calculate the size of the exterior (outer) ring
                let exterior_size = polygon.exterior().0.capacity() * size_of::<f64>() * 2; // times 2 for x, y coordinates
                                                                                            // Sum the sizes of the interiors (inner rings/holes)
                let interiors_size = polygon
                    .interiors()
                    .iter()
                    .map(|line_string| {
                        line_string.0.capacity() * size_of::<f64>() * 2
                        // times 2 for x, y coordinates
                    })
                    .sum::<usize>();
                // Return the total size for this polygon
                exterior_size + interiors_size
            })
            .sum::<usize>();

        // Return the total size of the MultiPolygonBody
        region_size + multipolygon_size
    }
}
