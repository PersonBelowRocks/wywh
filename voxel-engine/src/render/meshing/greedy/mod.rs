use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::{
    data::{
        registries::{
            texture::TextureRegistry,
            variant::{VariantRegistry, VariantRegistryEntry},
            RegistryId, RegistryRef,
        },
        tile::{Face, Transparency},
    },
    render::{adjacency::AdjacentTransparency, quad::data::DataQuad},
    topo::{
        access::{ChunkBounds, ReadAccess},
        chunk::Chunk,
        chunk_ref::ChunkVoxelOutput,
    },
    util::Axis3D,
};

use super::ChunkAccess;

pub mod algorithm;
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
pub struct ChunkQuadSlice<'a, C: ChunkAccess> {
    face: Face,
    mag: i32,

    access: &'a C,
    adjacent: &'a AdjacentTransparency,
    registry: &'a RegistryRef<'a, VariantRegistry>,
}

#[derive(Clone, Debug)]
pub struct GreedyQuadData {
    occluded: bool,
    texture: RegistryId<TextureRegistry>,
}

pub const MAX: IVec2 = IVec2::splat(Chunk::SIZE);

#[allow(private_bounds)]
impl<'a, C: ChunkAccess> ChunkQuadSlice<'a, C> {
    pub fn contains(pos: IVec2) -> bool {
        pos.cmplt(MAX).all() && pos.cmpge(IVec2::ZERO).all()
    }

    pub fn pos_3d(&self, pos: IVec2) -> IVec3 {
        ivec_project_to_3d(pos, self.face, self.mag)
    }

    pub fn pos_2d(&self, pos: IVec3) -> IVec2 {
        ivec_project_to_2d(pos, self.face)
    }

    pub fn get_transparency_above(&self, pos: IVec2) -> Transparency {
        todo!()
    }

    pub fn get_quad(&self, pos: IVec2) -> DataQuad<GreedyQuadData> {
        todo!()
    }
}
