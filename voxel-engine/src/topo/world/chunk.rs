use std::fmt;

use bevy::math::ivec3;
use bevy::prelude::*;
use bitflags::bitflags;

use octo::SubdividedStorage;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::data::registries::block::{BlockVariantId, BlockVariantRegistry};
use crate::data::registries::Registry;
use crate::data::voxel::rotations::BlockModelRotation;
use crate::topo::block::{BlockVoxel, FullBlock, SubdividedBlock};
use crate::topo::bounding_box::BoundingBox;
use crate::topo::controller::{LoadReasons, LoadshareMap};

use super::{ChunkDataError, ChunkHandleError};

#[derive(dm::From, dm::Into, dm::Display, Debug, PartialEq, Eq, Hash, Copy, Clone, Component)]
pub struct ChunkPos(IVec3);

impl ChunkPos {
    pub const ZERO: Self = Self(IVec3::ZERO);

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self(ivec3(x, y, z))
    }

    /// The corner of this chunk closest to +infinity
    /// For a `ChunkPos` of `[0, 0, 0]` this would be `[15, 15, 15]`.
    pub fn worldspace_max(self) -> IVec3 {
        (self.0 * Chunk::SIZE) + (Chunk::SIZE - 1)
    }

    /// The corner of this chunk closest to -infinity
    /// For a `ChunkPos` of `[0, 0, 0]` this would be `[0, 0, 0]`.
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
    /// Flags that describe various properties of a chunk
    #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    pub struct ChunkFlags: u32 {
        /// Indicates that the chunk is currently being populated by the world generator.
        const GENERATING = 0b1 << 0;
        /// Indicates that the chunk should be remeshed, when the engine remeshes the chunk this flag will
        /// be unset.
        const REMESH = 0b1 << 1;
        // TODO: have flags for each edge that was updated
        /// Indicates that the chunk's neighbors should be remeshed
        const REMESH_NEIGHBORS = 0b1 << 2;
        /// Indicates that this chunk was just generated and has not been meshed before
        const FRESHLY_GENERATED = 0b1 << 3;
        /// Indicates that a chunk has not been populated with the generator and is only really
        /// acting as a "dummy" until it's further processed by the engine.
        /// Chunks are not supposed to be primordial for long, primordial chunks are usually immediately
        /// queued for further processing by the engine to get them out of their primordial state.
        const PRIMORDIAL = 0b1 << 4;
    }
}

impl fmt::Debug for ChunkFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let permit_flag_names = [
            (Self::GENERATING, "GENERATING"),
            (Self::REMESH, "REMESH"),
            (Self::REMESH_NEIGHBORS, "REMESH_NEIGHBORS"),
            (Self::FRESHLY_GENERATED, "FRESHLY_GENERATED"),
            (Self::PRIMORDIAL, "PRIMORDIAL"),
        ];

        let mut list = f.debug_list();

        for (flag, name) in permit_flag_names {
            if self.contains(flag) {
                list.entry(&name);
            }
        }

        list.finish()
    }
}

#[derive(Copy, Clone, Debug, Component, PartialEq, Eq)]
pub struct ChunkEntity;

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, dm::Constructor)]
pub struct VoxelVariantData {
    pub variant: <BlockVariantRegistry as Registry>::Id,
    pub rotation: Option<BlockModelRotation>,
}

#[derive(Clone)]
pub struct ChunkLoadReasons {
    pub loadshares: LoadshareMap<LoadReasons>,
    pub cached_reasons: LoadReasons,
}

impl ChunkLoadReasons {
    /// Updates the cached load reasons and returns them.
    /// Should be called whenever a loadshare updates load reasons.
    pub fn update_cached_reasons(&mut self) -> LoadReasons {
        let mut cached = LoadReasons::empty();

        for &reasons in self.loadshares.values() {
            cached |= reasons;
        }

        self.cached_reasons = cached;
        cached
    }
}

/// The full-block dimensions of a chunk
pub const CHUNK_SIZE: usize = 16;
/// Dimensions of a subdivided block
pub const BLOCK_SUBDIVISIONS: usize = 4;
/// The maximum value that can safely be stored in [`ChunkData`].
pub const MAX_ALLOWED_CHUNK_DATA_VALUE: u32 = (0b1 << (u32::BITS - 1)) - 1;

pub type ChunkDataStorage = octo::SubdividedStorage<CHUNK_SIZE, BLOCK_SUBDIVISIONS, u32>;

/// The actual voxel data of a chunk
#[derive(Clone)]
pub struct ChunkData {
    default: u32,
    storage: Option<ChunkDataStorage>,
}

impl ChunkData {
    /// Checks if the chunk data contains the given full-block position.
    /// # Vectors
    /// [`ls_pos`] is in full-block localspace.
    #[inline]
    pub fn contains_full_block(ls_pos: IVec3) -> bool {
        ls_pos.cmpge(IVec3::ZERO).all() && ls_pos.cmplt(IVec3::splat(CHUNK_SIZE as _)).all()
    }

    /// Checks if the chunk data contains the given microblock position.
    /// # Vectors
    /// [`mb_pos`] is in microblock localspace.
    #[inline]
    pub fn contains_microblock(mb_pos: IVec3) -> bool {
        mb_pos.cmpge(IVec3::ZERO).all()
            && mb_pos
                .cmplt(IVec3::splat((CHUNK_SIZE * BLOCK_SUBDIVISIONS) as _))
                .all()
    }

    /// Create new chunk data with the provided default value. All reads from this data
    /// will return that default value until anything else is written.
    pub fn new(default: u32) -> Self {
        Self {
            default,
            storage: None,
        }
    }

    /// Initialize this chunk data. Returns whether or not the storage was actually initalized,
    /// i.e., returns false if the storage was initialized, and true if not.
    /// The chunk data should have identical reading behaviour after this function is called,
    /// this function is just here so that you can have more manual control of allocations.
    #[inline]
    pub fn touch(&mut self) -> bool {
        match self.storage {
            None => {
                self.storage = Some(SubdividedStorage::new(self.default));
                true
            }
            Some(_) => false,
        }
    }

    /// Get the value at the given full-block position, returning an error if the position
    /// is out of bounds or if the block at the position was subdivided.
    /// # Vectors
    /// [`ls_pos`] is in full-block localspace.
    #[inline]
    pub fn get(&self, ls_pos: IVec3) -> Result<u32, ChunkDataError> {
        if !Self::contains_full_block(ls_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        self.storage.as_ref().map_or(Ok(self.default), |storage| {
            let index = ls_pos.to_array().map(|v| v as u8);
            storage.get(index).map_err(ChunkDataError::from)
        })
    }

    /// Set the value at the given full-block localspace position.
    /// Returns an error if the position is out of bounds.
    /// If this is the first time writing to this chunk and the written value is not the default value,
    /// it will be initialized, which can cause memory allocation.
    /// # Vectors
    /// [`ls_pos`] is in full-block localspace.
    #[inline]
    pub fn set(&mut self, ls_pos: IVec3, value: u32) -> Result<(), ChunkDataError> {
        if !Self::contains_full_block(ls_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        // Writing is pointless if the value is the default.
        if value == self.default {
            return Ok(());
        }

        let storage = self
            .storage
            .get_or_insert_with(|| ChunkDataStorage::new(self.default));

        let index = ls_pos.to_array().map(|v| v as u8);
        Ok(storage.set(index, value)?)
    }

    /// Get the value at the given microblock position, returning an error if the position
    /// is out of bounds.
    /// # Vectors
    /// [`mb_pos`] is in microblock localspace.
    #[inline]
    pub fn get_mb(&self, mb_pos: IVec3) -> Result<u32, ChunkDataError> {
        if !Self::contains_microblock(mb_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        self.storage.as_ref().map_or(Ok(self.default), |storage| {
            let index = mb_pos.to_array().map(|v| v as u8);
            storage.get_mb(index).map_err(ChunkDataError::from)
        })
    }

    /// Set the value at the given microblock localspace position.
    /// Returns an error if the position is out of bounds.
    /// If this is the first time writing to this chunk and the written value is not the default value,
    /// it will be initialized, which can cause memory allocation.
    /// ### Microblock allocations
    /// Writing microblocks can cause allocations in another way too, if a microblock is written
    /// to a position that currently contains a full-block, then that full-block has to be
    /// subdivided and moved into the subdivided block buffer, potentially allocating.
    /// # Vectors
    /// [`mb_pos`] is in microblock localspace.
    #[inline]
    pub fn set_mb(&mut self, mb_pos: IVec3, value: u32) -> Result<(), ChunkDataError> {
        if !Self::contains_microblock(mb_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        // Writing is pointless if the value is the default.
        if value == self.default {
            return Ok(());
        }

        let storage = self
            .storage
            .get_or_insert_with(|| ChunkDataStorage::new(self.default));

        let index = mb_pos.to_array().map(|v| v as u8);
        Ok(storage.set_mb(index, value)?)
    }
}

/// Read-only handle to a chunk's data. Essentially a read guard for the chunk's data lock.
pub struct ChunkReadHandle<'a> {
    flags: RwLockReadGuard<'a, ChunkFlags>,
    blocks: RwLockReadGuard<'a, ChunkData>,
}

impl<'a> ChunkReadHandle<'a> {
    /// Get the value at the given full-block position.
    /// Errors on any of these conditions:
    /// - The position is out of bounds
    /// - The block at the position is not a full-block
    /// - The value at the position cannot be made into a [`BlockVariantId`]
    /// # Vectors
    /// [`ls_pos`] is in full-block localspace.
    #[inline]
    pub fn get(&self, ls_pos: IVec3) -> Result<BlockVariantId, ChunkHandleError> {
        let raw = self.blocks.get(ls_pos)?;

        if raw > MAX_ALLOWED_CHUNK_DATA_VALUE {
            return Err(ChunkHandleError::InvalidDataValue(raw));
        }

        let variant_id = unsafe { BlockVariantId::from_raw(raw) };
        Ok(variant_id)
    }

    /// Get the variant ID at the given microblock position.
    /// Errors on any of these conditions:
    ///  - The position is out of bounds
    ///  - The value at the position cannot be made into a [`BlockVariantId`]
    /// # Vectors
    /// [`mb_pos`] is in microblock localspace.
    #[inline]
    pub fn get_mb(&self, mb_pos: IVec3) -> Result<BlockVariantId, ChunkHandleError> {
        let raw = self.blocks.get_mb(mb_pos)?;

        if raw > MAX_ALLOWED_CHUNK_DATA_VALUE {
            return Err(ChunkHandleError::InvalidDataValue(raw));
        }

        let variant_id = unsafe { BlockVariantId::from_raw(raw) };
        Ok(variant_id)
    }

    // TODO: docs
    #[inline]
    pub fn get_block(&self, ls_pos: IVec3) -> Result<(), ChunkDataError> {
        todo!()
    }
}

pub struct ChunkWriteHandle<'a> {
    flags: RwLockWriteGuard<'a, ChunkFlags>,
    blocks: RwLockWriteGuard<'a, ChunkData>,
}

impl<'a> ChunkWriteHandle<'a> {
    // TODO: methods!
}

pub struct Chunk {
    pub flags: RwLock<ChunkFlags>,
    pub load_reasons: RwLock<ChunkLoadReasons>,
    pub blocks: RwLock<ChunkData>,
}

#[allow(dead_code)]
impl Chunk {
    pub const USIZE: usize = CHUNK_SIZE;
    pub const SIZE: i32 = Self::USIZE as i32;
    pub const SIZE_LOG2: u32 = Self::SIZE.ilog2();

    pub const SUBDIVIDED_CHUNK_SIZE: i32 = SubdividedBlock::SUBDIVISIONS * Self::SIZE;
    pub const SUBDIVIDED_CHUNK_USIZE: usize = Self::SUBDIVIDED_CHUNK_SIZE as usize;

    pub const VEC: IVec3 = IVec3::splat(Self::SIZE);

    pub const BOUNDING_BOX: BoundingBox = BoundingBox {
        min: IVec3::splat(0),
        max: IVec3::splat(Self::SIZE),
    };

    #[inline]
    pub fn new(
        filling: BlockVariantId,
        initial_flags: ChunkFlags,
        load_reasons: ChunkLoadReasons,
    ) -> Self {
        Self {
            flags: RwLock::new(initial_flags),
            load_reasons: RwLock::new(load_reasons),
            blocks: RwLock::new(ChunkData::new(filling.as_u32())),
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
