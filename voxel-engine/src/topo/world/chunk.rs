use bevy::prelude::*;
use bitflags::bitflags;

use parking_lot::RwLock;

use crate::data::registries::block::BlockVariantRegistry;
use crate::data::registries::Registry;
use crate::data::voxel::rotations::BlockModelRotation;
use crate::topo::block::{BlockVoxel, SubdividedBlock};
use crate::topo::bounding_box::BoundingBox;
use crate::topo::storage::containers::data_storage::SyncIndexedChunkContainer;

#[derive(dm::From, dm::Into, dm::Display, Debug, PartialEq, Eq, Hash, Copy, Clone, Component)]
pub struct ChunkPos(IVec3);

impl ChunkPos {
    pub const ZERO: Self = Self(IVec3::ZERO);

    pub const fn new(pos: IVec3) -> Self {
        Self(pos)
    }

    pub fn worldspace_max(self) -> IVec3 {
        (self.0 * Chunk::SIZE) + (Chunk::SIZE - 1)
    }

    pub fn worldspace_min(self) -> IVec3 {
        self.0 * Chunk::SIZE
    }

    pub fn x(self) -> i32 {
        self.0.x
    }

    pub fn y(self) -> i32 {
        self.0.y
    }

    pub fn z(self) -> i32 {
        self.0.z
    }

    pub fn as_ivec3(self) -> IVec3 {
        self.0
    }

    pub fn as_vec3(self) -> Vec3 {
        self.0.as_vec3()
    }
}

bitflags! {
    #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    pub struct ChunkFlags: u32 {
        const GENERATING = 0b1 << 0;
        const REMESH = 0b1 << 1;
        // TODO: have flags for each edge that was updated
        const REMESH_NEIGHBORS = 0b1 << 2;
        const FRESH = 0b1 << 3;
    }
}

#[derive(Copy, Clone, Debug, Component, PartialEq, Eq)]
pub struct ChunkEntity;

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, dm::Constructor)]
pub struct VoxelVariantData {
    pub variant: <BlockVariantRegistry as Registry>::Id,
    pub rotation: Option<BlockModelRotation>,
}

pub struct Chunk {
    pub flags: RwLock<ChunkFlags>,
    pub variants: SyncIndexedChunkContainer<BlockVoxel>,
}

const CHUNK_SIZE: usize = 16;

#[allow(dead_code)]
impl Chunk {
    pub const USIZE: usize = CHUNK_SIZE;
    pub const SIZE: i32 = Self::USIZE as i32;

    pub const SUBDIVIDED_CHUNK_SIZE: i32 = SubdividedBlock::SUBDIVISIONS * Self::SIZE;
    pub const SUBDIVIDED_CHUNK_USIZE: usize = Self::SUBDIVIDED_CHUNK_SIZE as usize;

    pub const VEC: IVec3 = IVec3::splat(Self::SIZE);

    pub const BOUNDING_BOX: BoundingBox = BoundingBox {
        min: IVec3::splat(0),
        max: IVec3::splat(Self::SIZE),
    };

    #[inline]
    pub fn new(filling: BlockVoxel, initial_flags: ChunkFlags) -> Self {
        Self {
            flags: RwLock::new(initial_flags),
            variants: SyncIndexedChunkContainer::filled(filling),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn chunkpos_to_worldspace() {
        fn test(chunk_pos_splat: i32, min_splat: i32, max_splat: i32) {
            let chunk_pos = ChunkPos::from(IVec3::splat(chunk_pos_splat));

            assert_eq!(chunk_pos.worldspace_min(), IVec3::splat(min_splat));
            assert_eq!(chunk_pos.worldspace_max(), IVec3::splat(max_splat));

            let mut count = 0;
            for _ in min_splat..=max_splat {
                count += 1
            }

            assert_eq!(count, 16)
        }

        test(0, 0, 15);
        test(1, 16, 31);
        test(2, 32, 47);
        test(-1, -16, -1);
        test(-2, -32, -17);
    }
}
