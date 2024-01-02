use bevy::math::IVec3;

use crate::{
    data::tile::Face,
    topo::{
        access::{ChunkBounds, ReadAccess},
        chunk::ChunkPos,
        chunk_ref::ChunkVoxelOutput,
        realm::ChunkManager,
    },
    util::FaceMap,
};

pub mod greedy;
pub mod immediate;

pub trait ChunkAccess: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}
impl<T> ChunkAccess for T where T: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}

#[derive(Clone)]
pub struct Neighbors<'a, C: ChunkAccess> {
    accesses: FaceMap<Option<&'a C>>,
}

impl<'a, C: ChunkAccess> Neighbors<'a, C> {
    pub fn new(pos: ChunkPos, manager: &ChunkManager) -> Result<Self, ()> {
        let map = FaceMap::<Option<&C>>::filled(None);

        for face in Face::FACES {
            let adjacent_pos = ChunkPos::from(IVec3::from(pos) + face.normal());
            let Some(cref) = manager.get_loaded_chunk(adjacent_pos).ok() else {
                continue;
            };

            todo!()
        }

        todo!()
    }
}
