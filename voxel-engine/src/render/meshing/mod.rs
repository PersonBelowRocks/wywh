use std::array;

use bevy::math::{ivec3, BVec3, IVec2, IVec3};
use itertools::Itertools;

use crate::{
    data::tile::Face,
    topo::{
        access::{ChunkBounds, ReadAccess},
        chunk::{Chunk, ChunkPos},
        chunk_ref::ChunkVoxelOutput,
        realm::ChunkManager,
        storage::error::OutOfBounds,
    },
    util::{CubicArray, FaceMap},
};

pub mod greedy;
pub mod immediate;

pub trait ChunkAccess: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}
impl<T> ChunkAccess for T where T: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}

#[derive(Clone)]
pub struct Neighbors<C: ChunkAccess> {
    pos: ChunkPos,
    chunks: [Option<C>; 3 * 3 * 3],
}

impl<C: ChunkAccess> Neighbors<C> {
    fn to_1d(x: usize, y: usize, z: usize) -> usize {
        const MAX: usize = 3;
        return (z * MAX * MAX) + (y * MAX) + x;
    }

    fn chkspace_pos_to_chk_pos(pos: IVec3) -> IVec3 {
        ivec3(
            pos.x.div_euclid(Chunk::SIZE),
            pos.y.div_euclid(Chunk::SIZE),
            pos.z.div_euclid(Chunk::SIZE),
        )
    }

    fn place_chkspace_pos_origin_on_neighbor_origin(pos: IVec3) -> IVec3 {
        ivec3(
            pos.x.rem_euclid(Chunk::SIZE),
            pos.y.rem_euclid(Chunk::SIZE),
            pos.z.rem_euclid(Chunk::SIZE),
        )
    }

    /// `pos` is in chunkspace
    fn internal_get(&self, pos: IVec3) -> Result<(), OutOfBounds> {
        let chk_pos = Self::chkspace_pos_to_chk_pos(pos);

        if chk_pos == IVec3::ZERO {
            // tried to access center chunk (aka. the chunk for which we represent the neighbors)
            return Err(OutOfBounds);
        }

        todo!()
    }

    pub fn get(&self, face: Face, pos: IVec2) -> Result<(), OutOfBounds> {
        todo!()
    }
}

#[derive(Clone)]
pub struct NeighborsBuilder<C: ChunkAccess>(Neighbors<C>);

impl<C: ChunkAccess> NeighborsBuilder<C> {
    pub fn new(pos: ChunkPos) -> Self {
        Self(Neighbors {
            pos,
            chunks: Default::default(),
        })
    }

    pub fn set_neighbor(&mut self, pos: IVec3, access: C) -> Result<(), OutOfBounds> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use crate::topo::access;

    use super::*;

    struct DummyAccess;

    impl ChunkBounds for DummyAccess {}
    impl access::ReadAccess for DummyAccess {
        type ReadErr = OutOfBounds;
        type ReadType = ChunkVoxelOutput;

        fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
            panic!()
        }
    }

    #[test]
    fn test_chunkspace_to_chunk_pos() {
        // for readability's sake
        fn f(x: i32, y: i32, z: i32) -> IVec3 {
            Neighbors::<DummyAccess>::chkspace_pos_to_chk_pos(ivec3(x, y, z))
        }

        assert_eq!(ivec3(0, 0, 0), f(8, 5, 6));
        assert_eq!(ivec3(0, 0, 0), f(0, 0, 0));
        assert_eq!(ivec3(0, 0, 0), f(15, 15, 15));
        assert_eq!(ivec3(0, 0, 1), f(15, 15, 16));
        assert_eq!(ivec3(0, -1, 0), f(0, -1, 0));
        assert_eq!(ivec3(0, -1, 1), f(0, -1, 16));
    }

    #[test]
    fn test_move_pos_origin() {
        // for readability's sake
        fn f(x: i32, y: i32, z: i32) -> IVec3 {
            Neighbors::<DummyAccess>::place_chkspace_pos_origin_on_neighbor_origin(ivec3(x, y, z))
        }

        assert_eq!(ivec3(5, 5, 5), f(5, 5, 5));
        assert_eq!(ivec3(0, 0, 0), f(0, 0, 0));
        assert_eq!(ivec3(0, 15, 0), f(0, -1, 0));
        assert_eq!(ivec3(0, 0, 5), f(0, 16, 5));
    }
}
