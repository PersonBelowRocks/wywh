use std::array;

use bevy::prelude::IVec3;

use bevy::prelude::UVec2;

use crate::data::tile::Face;

use super::vertex::VoxelFaceVertexData;

#[derive(Copy, Clone, Debug)]
pub(crate) struct FaceMesh {
    pub face: Face,
    pub vxl_pos: IVec3,
    pub tex: UVec2,
}

impl FaceMesh {
    pub fn add_to_mesh(self, data: &mut Vec<u32>, indices: &mut Vec<u32>, current_idx: &mut u32) {
        let corners: [VoxelFaceVertexData; 4] = array::from_fn(|i| {
            // let idx = if self.face.axis_direction() < 0 {
            //     // ((i as i32) - 3).unsigned_abs()
            //     i as u32
            // } else {
            //     i as u32
            // };

            self.corner(i as u32)
        });

        let indices_pattern = [0u32, 1, 2, 3, 2, 1]
            .into_iter()
            .map(|idx| idx + *current_idx);

        match self.face {
            Face::Bottom | Face::North | Face::East => indices.extend(indices_pattern.rev()),
            _ => indices.extend(indices_pattern),
        }
        // indices.extend(indices_pattern);
        data.extend(corners.map(|v| v.pack().unwrap()));

        *current_idx += 4;
    }

    fn corner(self, corner: u32) -> VoxelFaceVertexData {
        VoxelFaceVertexData {
            face: self.face,
            vxl_pos: self.vxl_pos,
            texture_pos: self.tex,
            corner,
        }
    }
}
