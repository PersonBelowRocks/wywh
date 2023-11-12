use bevy::prelude::*;

use super::bounding_box::BoundingBox;
use super::storage::containers::{RawChunkVoxelContainer, SyncChunkVoxelContainer};
use super::storage::data_structures::ChunkVoxelDataStorage;
use crate::data::tile::VoxelId;

const CHUNK_SIZE: usize = 16;

#[derive(
    dm::From, dm::Into, dm::Display, Debug, PartialEq, Eq, Hash, Copy, Clone, Deref, DerefMut,
)]
pub struct ChunkPos(IVec3);

impl ChunkPos {
    pub fn worldspace_max(self) -> IVec3 {
        (self.0 * Chunk::SIZE) + (Chunk::SIZE - 1)
    }

    pub fn worldspace_min(self) -> IVec3 {
        self.0 * Chunk::SIZE
    }
}

pub struct Chunk {
    pub voxels: SyncChunkVoxelContainer<VoxelId>,
}

#[allow(dead_code)]
impl Chunk {
    pub const USIZE: usize = CHUNK_SIZE;
    pub const SIZE: i32 = Self::USIZE as i32;

    pub const BOUNDING_BOX: BoundingBox = BoundingBox {
        min: IVec3::splat(0),
        max: IVec3::splat(Self::SIZE),
    };

    #[inline]
    pub fn new(voxel_data: ChunkVoxelDataStorage<VoxelId>) -> Self {
        Self {
            voxels: SyncChunkVoxelContainer::new(voxel_data),
        }
    }

    #[inline]
    pub fn new_from_container(container: RawChunkVoxelContainer<VoxelId>) -> Self {
        Self {
            voxels: SyncChunkVoxelContainer(container.into()),
        }
    }

    #[inline]
    pub fn empty() -> Self {
        Self {
            voxels: SyncChunkVoxelContainer::empty(),
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
