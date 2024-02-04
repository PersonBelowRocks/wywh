use bevy::prelude::*;

use super::bounding_box::BoundingBox;
use super::storage::containers::data_storage::SyncIndexedChunkContainer;
use super::storage::containers::dense::{DenseChunkContainer, SyncDenseChunkContainer};
use super::storage::data_structures::DenseChunkStorage;
use crate::data::registries::variant::VariantRegistry;
use crate::data::registries::RegistryId;
use crate::data::tile::Transparency;
use crate::data::voxel::rotations::BlockModelRotation;

#[derive(
    dm::From,
    dm::Into,
    dm::Display,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Copy,
    Clone,
    Deref,
    DerefMut,
    Component,
)]
pub struct ChunkPos(IVec3);

#[derive(Copy, Clone, Debug, Component, PartialEq, Eq)]
pub struct ChunkEntity;

impl ChunkPos {
    pub const ZERO: Self = Self(IVec3::ZERO);

    pub fn worldspace_max(self) -> IVec3 {
        (self.0 * Chunk::SIZE) + (Chunk::SIZE - 1)
    }

    pub fn worldspace_min(self) -> IVec3 {
        self.0 * Chunk::SIZE
    }
}

pub type VariantType = RegistryId<VariantRegistry>;

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, dm::Constructor)]
pub struct VoxelVariantData {
    pub variant: RegistryId<VariantRegistry>,
    pub rotation: Option<BlockModelRotation>,
}

pub struct Chunk {
    pub transparency: SyncDenseChunkContainer<Transparency>,
    pub variants: SyncIndexedChunkContainer<VoxelVariantData>,
}

const CHUNK_SIZE: usize = 16;

#[allow(dead_code)]
impl Chunk {
    pub const USIZE: usize = CHUNK_SIZE;
    pub const SIZE: i32 = Self::USIZE as i32;
    pub const VEC: IVec3 = IVec3::splat(Self::SIZE);

    pub const BOUNDING_BOX: BoundingBox = BoundingBox {
        min: IVec3::splat(0),
        max: IVec3::splat(Self::SIZE),
    };

    #[inline]
    pub fn new(voxel_data: DenseChunkStorage<Transparency>) -> Self {
        Self {
            transparency: SyncDenseChunkContainer::new(voxel_data),
            variants: SyncIndexedChunkContainer::new(),
        }
    }

    #[inline]
    pub fn new_from_container(container: DenseChunkContainer<Transparency>) -> Self {
        Self {
            transparency: SyncDenseChunkContainer(container.into()),
            variants: SyncIndexedChunkContainer::new(),
        }
    }

    #[inline]
    pub fn empty() -> Self {
        Self {
            transparency: SyncDenseChunkContainer::empty(),
            variants: SyncIndexedChunkContainer::new(),
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
