use std::array;

use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::{
    data::{
        registries::{
            texture::TextureRegistry, variant::VariantRegistry, Registry, RegistryId, RegistryRef,
        },
        tile::{Face, Transparency},
        voxel::VoxelModel,
    },
    render::{
        adjacency::AdjacentTransparency,
        quad::{anon::Quad, data::DataQuad, isometric::QuadVertex},
    },
    topo::{
        access::ChunkAccess,
        chunk::Chunk,
        chunk_ref::ChunkVoxelOutput,
        neighbors::{self, Neighbors},
    },
    util::Axis3D,
};

use self::error::CqsError;

pub mod algorithm;
pub mod error;
pub mod greedy_mesh;
pub mod material;

#[inline]
pub fn ivec_project_to_3d(pos: IVec2, face: Face, mag: i32) -> IVec3 {
    match face.axis() {
        Axis3D::X => ivec3(mag, pos.y, pos.x),
        Axis3D::Y => ivec3(pos.x, mag, pos.y),
        Axis3D::Z => ivec3(pos.x, pos.y, mag),
    }
}

#[inline]
pub fn ivec_project_to_2d(pos: IVec3, face: Face) -> IVec2 {
    match face.axis() {
        Axis3D::X => ivec2(pos.z, pos.y),
        Axis3D::Y => ivec2(pos.x, pos.z),
        Axis3D::Z => ivec2(pos.x, pos.y),
    }
}

#[derive(Clone)]
#[allow(private_bounds)]
pub struct ChunkQuadSlice<'a, C: ChunkAccess, Nb: ChunkAccess> {
    face: Face,
    mag: i32,

    access: C,
    neighbors: Neighbors<Nb>,
    registry: &'a RegistryRef<'a, VariantRegistry>,
}

pub const MAX: IVec2 = IVec2::splat(Chunk::SIZE);

#[allow(type_alias_bounds)]
pub type CqsResult<T, C: ChunkAccess, Nb: ChunkAccess> =
    Result<T, CqsError<C::ReadErr, Nb::ReadErr>>;

impl<'a, C: ChunkAccess, Nb: ChunkAccess> ChunkQuadSlice<'a, C, Nb> {
    pub fn contains(pos: IVec2) -> bool {
        pos.cmplt(MAX).all() && pos.cmpge(IVec2::ZERO).all()
    }

    pub fn contains_3d(pos: IVec3) -> bool {
        pos.cmplt(Chunk::VEC).all() && pos.cmpge(IVec3::ZERO).all()
    }

    /// Transforms a position from facespace to localspace
    #[inline]
    pub fn pos_3d(&self, pos: IVec2) -> IVec3 {
        ivec_project_to_3d(pos, self.face, self.mag)
    }

    /// Transforms a position from localspace to facespace
    #[inline]
    pub fn pos_2d(&self, pos: IVec3) -> IVec2 {
        ivec_project_to_2d(pos, self.face)
    }

    #[inline]
    pub fn get_3d(&self, pos: IVec3) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        self.access.get(pos).map_err(|e| CqsError::AccessError(e))
    }

    /// `pos` is in localspace and can exceed the regular chunk bounds by 1 for any component of the vector.
    /// In this case the `ChunkVoxelOutput` is taken from a neighboring chunk.
    #[inline]
    pub fn auto_neighboring_get(&self, pos: IVec3) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        if Self::contains_3d(pos) && !neighbors::is_in_bounds_3d(pos) {
            self.get_3d(pos)
        } else if !Self::contains_3d(pos) && neighbors::is_in_bounds_3d(pos) {
            Ok(self.neighbors.get_3d(pos)?)
        } else {
            return Err(CqsError::OutOfBounds);
        }
    }

    #[inline]
    pub fn get(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos3d = self.pos_3d(pos);
        self.get_3d(pos3d)
    }

    #[inline]
    pub fn get_above(&self, pos: IVec2) -> CqsResult<ChunkVoxelOutput, C, Nb> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        let pos_above = self.pos_3d(pos) + self.face.normal();
        self.auto_neighboring_get(pos_above)
    }

    fn corner_occlusions(&self, pos: IVec2, quad: &mut DataQuad) -> CqsResult<(), C, Nb> {
        let corners = [
            pos + ivec2(-1, 1),
            pos + ivec2(1, 1),
            pos + ivec2(-1, -1),
            pos + ivec2(1, -1),
        ];

        let is_corner_occluded = |i: usize| {
            let corner_pos = self.pos_3d(corners[i]) + self.face.normal();
            self.auto_neighboring_get(corner_pos)
                .map(|cvo| cvo.transparency.is_opaque())
        };

        let occlusions: [bool; 4] = [
            is_corner_occluded(0)?,
            is_corner_occluded(1)?,
            is_corner_occluded(2)?,
            is_corner_occluded(3)?,
        ];

        for vertex in QuadVertex::VERTICES {
            quad.data.get_mut(vertex).occluded = occlusions[vertex.as_usize()];
        }

        Ok(())
    }

    fn edge_occlusions(&self, pos: IVec2, quad: &mut DataQuad) -> CqsResult<(), C, Nb> {
        let edge_voxel = |offset: IVec2| self.pos_3d(pos + offset) + self.face.normal();

        let mut edge_occlusion = |offset: IVec2, pair: [QuadVertex; 2]| -> CqsResult<(), C, Nb> {
            let edge_vox_pos = edge_voxel(offset);
            let cvo = self.auto_neighboring_get(edge_vox_pos)?;

            for v in pair {
                quad.data.get_mut(v).occluded = cvo.transparency.is_opaque();
            }

            Ok(())
        };

        /*
            0---1
            |   |
            2---3
        */

        edge_occlusion(ivec2(0, 1), [QuadVertex::Zero, QuadVertex::One])?;
        edge_occlusion(ivec2(1, 0), [QuadVertex::One, QuadVertex::Three])?;
        edge_occlusion(ivec2(0, -1), [QuadVertex::Three, QuadVertex::Two])?;
        edge_occlusion(ivec2(-1, 0), [QuadVertex::Two, QuadVertex::Zero])?;

        Ok(())
    }

    #[inline]
    pub fn calculate_occlusion(&self, pos: IVec2, quad: &mut DataQuad) -> CqsResult<(), C, Nb> {
        if !Self::contains(pos) {
            return Err(CqsError::OutOfBounds);
        }

        self.corner_occlusions(pos, quad)?;
        self.edge_occlusions(pos, quad)?;

        Ok(())
    }

    #[inline(always)]
    pub fn get_quad(&self, pos: IVec2) -> CqsResult<Option<DataQuad>, C, Nb> {
        let cvo = self.get(pos)?;

        if cvo.transparency.is_transparent() || self.get_above(pos)?.transparency.is_opaque() {
            // nothing to see here if we're either transparent or the block above is obscuring us
            return Ok(None);
        }

        let variant = self.registry.get_by_id(cvo.variant);
        if let Some(VoxelModel::Block(model)) = variant.model {
            let submodel = match cvo.rotation {
                Some(rotation) => model.submodel(rotation.front()),
                None => model.default_submodel(),
            };

            let texture = submodel.get_texture(self.face);
            let mut quad = DataQuad::new(Quad::ONE, texture);

            // TODO: calculate occlusion
            self.calculate_occlusion(pos, &mut quad)?;

            Ok(Some(quad))
        } else {
            // can only get quads from a block model
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::topo::{
        access::{ChunkBounds, HasBounds, ReadAccess},
        storage::{data_structures::HashmapChunkStorage, error::OutOfBounds},
    };

    use super::*;

    struct TestAccess
    where
        Self: ChunkAccess,
    {
        map: HashmapChunkStorage<ChunkVoxelOutput>,
        default: ChunkVoxelOutput,
    }

    impl ChunkBounds for TestAccess {}
    impl ReadAccess for TestAccess {
        type ReadErr = OutOfBounds;
        type ReadType = ChunkVoxelOutput;

        fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
            if !self.bounds().contains(pos) {
                return Err(OutOfBounds);
            }

            Ok(self.map.get(pos).unwrap_or(self.default))
        }
    }

    #[test]
    fn test_reading() {
        todo!()
    }

    #[test]
    fn test_occlusion() {
        todo!()
    }
}
