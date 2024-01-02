use bevy::math::IVec2;

use crate::{data::{tile::Face, registries::{variant::{VariantRegistryEntry, VariantRegistry}, RegistryRef, texture::TextureRegistry, RegistryId}}, topo::{access::{ReadAccess, ChunkBounds}, chunk_ref::ChunkVoxelOutput}, render::{adjacency::AdjacentTransparency, quad::data::DataQuad}};

pub mod algorithm;
pub mod greedy_mesh;
pub mod material;

pub(crate) trait ChunkAccess: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}
impl<T> ChunkAccess for T where T: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}

#[derive(Clone)]
#[allow(private_bounds)]
pub struct ChunkQuadSlice<'a, C: ChunkAccess> {
    face: Face,
    mag: i32,

    slice: &'a C,
    adjacent: &'a AdjacentTransparency,
    registry: &'a RegistryRef<'a, VariantRegistry>
}

#[derive(Clone, Debug)]
pub struct GreedyQuadData {
    occluded: bool,
    texture: RegistryId<TextureRegistry>
}

#[allow(private_bounds)]
impl<'a, C: ChunkAccess> ChunkQuadSlice<'a, C> {
    pub fn get_quad(&self, pos: IVec2) -> DataQuad<GreedyQuadData> {
        todo!()
    }
}