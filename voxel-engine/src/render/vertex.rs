use bevy::prelude::{IVec3, UVec2};

use crate::data::tile::Face;

#[allow(clippy::inconsistent_digit_grouping, unused)]
pub mod consts {
    // Representation of a voxel face corner in the GPU vertex buffer.
    // The chunk shader does not use position, normal, and uv vectors like other shaders, but
    // rather because we're dealing with a bunch of cubes we can compress all that data
    // into a u32 to save a lot of space. This diagram shows what data is stored where in
    // the u32. The first few bits marked with "..." are unused but may be used in the future.
    //
    // |-32-bits------------------------------------|
    // 000[000][0000][0000][0000][000000][000000][00]
    // ... 0)   1)    2)    3)    4)      5)      6)
    //
    // 0) Voxel face
    pub const FACE_BITMASK: u32 = 0b0001_1100_0000_0000_0000_0000_0000_0000;
    pub const FACE_RSHIFT: u32 = 26;
    // 1) Voxel X
    pub const VXL_X_BITMASK: u32 = 0b0000_0011_1100_0000_0000_0000_0000_0000;
    pub const VXL_X_RSHIFT: u32 = FACE_RSHIFT - 4;
    // 2) Voxel Y
    pub const VXL_Y_BITMASK: u32 = 0b0000_0000_0011_1100_0000_0000_0000_0000;
    pub const VXL_Y_RSHIFT: u32 = VXL_X_RSHIFT - 4;
    // 3) Voxel Z
    pub const VXL_Z_BITMASK: u32 = 0b0000_0000_0000_0011_1100_0000_0000_0000;
    pub const VXL_Z_RSHIFT: u32 = VXL_Y_RSHIFT - 4;
    // 4) Texture atlas X
    pub const TEX_ATLAS_X_BITMASK: u32 = 0b0000_0000_0000_0000_0011_1111_0000_0000;
    pub const TEX_ATLAS_X_RSHIFT: u32 = VXL_Z_RSHIFT - 6;
    // 5) Texture atlas Y
    pub const TEX_ATLAS_Y_BITMASK: u32 = 0b0000_0000_0000_0000_0000_0000_1111_1100;
    pub const TEX_ATLAS_Y_RSHIFT: u32 = TEX_ATLAS_X_RSHIFT - 6;
    // 6) Corner
    pub const CORNER_BITMASK: u32 = 0b0000_0000_0000_0000_0000_0000_0000_0011;
    pub const CORNER_RSHIFT: u32 = 0;
}

#[derive(te::Error, Debug)]
pub enum VfvdError {
    #[error("Voxel X out of bounds")]
    VxlXOob,
    #[error("Voxel Y out of bounds")]
    VxlYOob,
    #[error("Voxel Z out of bounds")]
    VxlZOob,
    #[error("Tex atlas X out of bounds")]
    TexAtlasXOob,
    #[error("Tex atlas Y out of bounds")]
    TexAtlasYOob,
    #[error("Invalid corner ID")]
    InvalidCornerId,
}

#[derive(Copy, Clone, Debug)]
pub struct VoxelFaceVertexData {
    pub face: Face,
    pub vxl_pos: IVec3,
    pub texture_pos: UVec2,
    pub corner: u32,
}

impl VoxelFaceVertexData {
    const FACE_BITS: u32 = 3;
    const VXL_POS_COMPONENT_BITS: u32 = 4;
    const TEX_ATLAS_POS_COMPONENT_BITS: u32 = 6;
    const CORNER_BITS: u32 = 2;

    pub fn voxel_pos(&self) -> Result<[u32; 3], VfvdError> {
        let x: u32 = self.vxl_pos.x.try_into().map_err(|_| VfvdError::VxlXOob)?;
        let y: u32 = self.vxl_pos.y.try_into().map_err(|_| VfvdError::VxlYOob)?;
        let z: u32 = self.vxl_pos.z.try_into().map_err(|_| VfvdError::VxlZOob)?;

        let max = 2u32.pow(Self::VXL_POS_COMPONENT_BITS);
        if x > max {
            return Err(VfvdError::VxlXOob);
        }

        if y > max {
            return Err(VfvdError::VxlYOob);
        }

        if z > max {
            return Err(VfvdError::VxlZOob);
        }

        Ok([x, y, z])
    }

    pub fn texture_pos(&self) -> Result<[u32; 2], VfvdError> {
        let max = 2u32.pow(Self::TEX_ATLAS_POS_COMPONENT_BITS);
        let [x, y] = self.texture_pos.to_array();

        if x > max {
            return Err(VfvdError::TexAtlasXOob);
        }

        if y > max {
            return Err(VfvdError::TexAtlasYOob);
        }

        Ok([x, y])
    }

    pub fn corner(&self) -> Result<u32, VfvdError> {
        if self.corner > 2u32.pow(Self::CORNER_BITS) {
            return Err(VfvdError::InvalidCornerId);
        }

        Ok(self.corner)
    }

    #[inline]
    pub fn pack(self) -> Result<u32, VfvdError> {
        use num_traits::ToPrimitive;

        let mut out: u32 = 0;

        out |= (self.face.to_u32().unwrap()) << consts::FACE_RSHIFT;

        let [x, y, z] = self.voxel_pos()?;
        out |= x << consts::VXL_X_RSHIFT;
        out |= y << consts::VXL_Y_RSHIFT;
        out |= z << consts::VXL_Z_RSHIFT;

        let [tx, ty] = self.texture_pos()?;
        out |= tx << consts::TEX_ATLAS_X_RSHIFT;
        out |= ty << consts::TEX_ATLAS_Y_RSHIFT;

        let corner = self.corner()?;
        out |= corner << consts::CORNER_RSHIFT;

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{Mat3, Vec3, Vec3Swizzles};

    use super::*;

    #[test]
    fn swizzle_transforms() {
        let top_face_vertices: [Vec3; 4] = [
            [-0.5, 0.5, 0.5].into(),
            [0.5, 0.5, 0.5].into(),
            [-0.5, 0.5, -0.5].into(),
            [0.5, 0.5, -0.5].into(),
        ];

        let north_face_vertices = top_face_vertices.map(|v| v.yxz());
        let south_face_vertices = top_face_vertices.map(|v| Vec3::new(-v.y, v.x, v.z));

        assert_eq!(
            north_face_vertices,
            [
                [0.5, -0.5, 0.5].into(),
                [0.5, 0.5, 0.5].into(),
                [0.5, -0.5, -0.5].into(),
                [0.5, 0.5, -0.5].into(),
            ]
        );

        assert_eq!(
            south_face_vertices,
            north_face_vertices.map(|v| Vec3::new(-v.x, v.y, v.z))
        );
    }
}
