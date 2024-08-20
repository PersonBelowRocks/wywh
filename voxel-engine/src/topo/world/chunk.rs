use bevy::math::ivec3;
use bevy::prelude::*;
use bitflags::bitflags;
use octo::SubdividedStorage;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::time::Duration;

use crate::data::registries::block::{BlockVariantId, BlockVariantRegistry};
use crate::data::registries::Registry;
use crate::data::voxel::rotations::BlockModelRotation;
use crate::topo::block::{BlockVoxel, FullBlock, SubdividedBlock};
use crate::topo::bounding_box::BoundingBox;
use crate::topo::controller::{LoadReasons, LoadshareMap};

use super::{ChunkDataError, ChunkHandleError, ChunkSyncError};

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
    default_value: u32,
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
            default_value: default,
            storage: None,
        }
    }

    /// Initialize this chunk data. Returns whether the storage was actually initialized,
    /// i.e., returns false if the storage was initialized, and true if not.
    /// The chunk data should have identical reading behaviour after this function is called,
    /// this function is just here so that you can have more manual control of allocations.
    #[inline]
    pub fn touch(&mut self) -> bool {
        match self.storage {
            None => {
                self.storage = Some(SubdividedStorage::new(self.default_value));
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

        self.storage
            .as_ref()
            .map_or(Ok(self.default_value), |storage| {
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
        if value == self.default_value {
            return Ok(());
        }

        let storage = self
            .storage
            .get_or_insert_with(|| ChunkDataStorage::new(self.default_value));

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

        self.storage
            .as_ref()
            .map_or(Ok(self.default_value), |storage| {
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
        if value == self.default_value {
            return Ok(());
        }

        let storage = self
            .storage
            .get_or_insert_with(|| ChunkDataStorage::new(self.default_value));

        let index = mb_pos.to_array().map(|v| v as u8);
        Ok(storage.set_mb(index, value)?)
    }

    /// Returns a reference to the underlying storage from `octo`, allowing for lower level
    /// operations.
    #[inline]
    pub fn storage(&self) -> Option<&ChunkDataStorage> {
        self.storage.as_ref()
    }

    /// Returns a mutable reference to the underlying storage from `octo`, allowing for lower level
    /// operations.
    #[inline]
    pub fn storage_mut(&mut self) -> Option<&mut ChunkDataStorage> {
        self.storage.as_mut()
    }
}

macro_rules! impl_chunk_handle_reads {
    ($lt:lifetime, $name:ty) => {
        impl<$lt> $name {
            /// Get the block at the given full-block position.
            /// Returns [`None`] if the block at the given position is not a full-block.
            /// Errors on any of these conditions:
            /// - The position is out of bounds
            /// - The value at the position cannot be made into a [`BlockVariantId`]
            /// # Vectors
            /// [`ls_pos`] is in full-block localspace.
            #[inline]
            pub fn get(&self, ls_pos: IVec3) -> Result<Option<BlockVariantId>, ChunkHandleError> {
                let raw = match self.blocks.get(ls_pos) {
                    Ok(raw) => raw,
                    Err(ChunkDataError::OutOfBounds) => {
                        return Err(ChunkHandleError::FullBlockOutOfBounds(ls_pos))
                    }
                    Err(ChunkDataError::NonFullBlock) => return Ok(None),
                };

                if raw > MAX_ALLOWED_CHUNK_DATA_VALUE {
                    return Err(ChunkHandleError::InvalidDataValue(raw));
                }

                let variant_id = unsafe { BlockVariantId::from_raw(raw) };
                Ok(Some(variant_id))
            }

            /// Get the variant ID at the given microblock position.
            /// Errors on any of these conditions:
            ///  - The position is out of bounds
            ///  - The value at the position cannot be made into a [`BlockVariantId`]
            /// # Vectors
            /// [`mb_pos`] is in microblock localspace.
            #[inline]
            pub fn get_mb(&self, mb_pos: IVec3) -> Result<BlockVariantId, ChunkHandleError> {
                let raw = match self.blocks.get_mb(mb_pos) {
                    Ok(raw) => raw,
                    Err(ChunkDataError::OutOfBounds) => {
                        return Err(ChunkHandleError::MicroblockOutOfBounds(mb_pos))
                    }
                    Err(ChunkDataError::NonFullBlock) => unreachable!(
                        "reading microblocks from a subdivided block is okay and intended"
                    ),
                };

                if raw > MAX_ALLOWED_CHUNK_DATA_VALUE {
                    return Err(ChunkHandleError::InvalidDataValue(raw));
                }

                let variant_id = unsafe { BlockVariantId::from_raw(raw) };
                Ok(variant_id)
            }

            /// Returns the inner chunk data, which allows for more low-level operations.
            #[inline]
            pub fn inner_ref(&self) -> &ChunkData {
                self.blocks.deref()
            }
        }
    };
}

/// Read-only handle to a chunk's data. Essentially a read guard for the chunk's data lock.
pub struct ChunkReadHandle<'a> {
    blocks: RwLockReadGuard<'a, ChunkData>,
}

impl_chunk_handle_reads!('a, ChunkReadHandle<'a>);

/// Read/Write handle to a chunk's data. Essentially a write guard for the chunk's data lock.
pub struct ChunkWriteHandle<'a> {
    flags: RwLockWriteGuard<'a, ChunkFlags>,
    blocks: RwLockWriteGuard<'a, ChunkData>,
}

impl_chunk_handle_reads!('a, ChunkWriteHandle<'a>);

impl<'a> ChunkWriteHandle<'a> {
    /// Set the value at the given full-block position.
    /// Errors on any of these conditions:
    ///  - The position is out of bounds
    ///  - The provided [`BlockVariantId`] is invalid.
    /// # Vectors
    /// [`ls_pos`] is in full-block localspace.
    #[inline]
    pub fn set(&mut self, ls_pos: IVec3, id: BlockVariantId) -> Result<(), ChunkHandleError> {
        let raw = id.as_u32();

        if raw > MAX_ALLOWED_CHUNK_DATA_VALUE {
            return Err(ChunkHandleError::InvalidDataValue(raw));
        }

        self.blocks.set(ls_pos, raw).map_err(|err| match err {
            ChunkDataError::NonFullBlock => unreachable!(
                "it should not be possible to encounter a non-full-block error when writing"
            ),
            ChunkDataError::OutOfBounds => ChunkHandleError::FullBlockOutOfBounds(ls_pos),
        })
    }

    /// Get the variant ID at the given microblock position.
    /// Errors on any of these conditions:
    ///  - The position is out of bounds
    ///  - The provided [`BlockVariantId`] is invalid.
    /// # Vectors
    /// [`mb_pos`] is in microblock localspace.
    #[inline]
    pub fn set_mb(&mut self, mb_pos: IVec3, id: BlockVariantId) -> Result<(), ChunkHandleError> {
        let raw = id.as_u32();

        if raw > MAX_ALLOWED_CHUNK_DATA_VALUE {
            return Err(ChunkHandleError::InvalidDataValue(raw));
        }

        self.blocks.set_mb(mb_pos, raw).map_err(|err| match err {
            ChunkDataError::NonFullBlock => unreachable!(
                "it should not be possible to encounter a non-full-block error when writing"
            ),
            ChunkDataError::OutOfBounds => ChunkHandleError::MicroblockOutOfBounds(mb_pos),
        })
    }

    /// Initializes the underlying data for writing. See [`ChunkData::touch`].
    #[inline]
    pub fn touch(&mut self) -> bool {
        self.blocks.touch()
    }

    /// Returns the inner chunk data, which allows for more low-level operations.
    #[inline]
    pub fn inner_mut(&mut self) -> &mut ChunkData {
        self.blocks.deref_mut()
    }
}

pub struct Chunk {
    chunk_pos: ChunkPos,
    pub flags: RwLock<ChunkFlags>,
    pub load_reasons: RwLock<ChunkLoadReasons>,
    pub blocks: RwLock<ChunkData>,
}

/// Describes the strategy that should be used when getting a lock over chunk data.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LockStrategy {
    /// Block for the given duration while waiting for a lock, and error if we exceed the timeout.
    Timeout(Duration),
    /// Block indefinitely while waiting for a lock.
    Blocking,
    /// Immediately get a lock to the data if possible, otherwise error.
    Immediate,
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
        chunk_pos: ChunkPos,
        filling: BlockVariantId,
        initial_flags: ChunkFlags,
        load_reasons: ChunkLoadReasons,
    ) -> Self {
        Self {
            chunk_pos,
            flags: RwLock::new(initial_flags),
            load_reasons: RwLock::new(load_reasons),
            blocks: RwLock::new(ChunkData::new(filling.as_u32())),
        }
    }

    #[inline]
    pub fn chunk_pos(&self) -> ChunkPos {
        self.chunk_pos
    }

    /// Get a read handle for this chunk with the given [lock strategy].
    ///
    /// The returned error depends on the lock strategy:
    /// - [`LockStrategy::Timeout`] will block and wait for the lock, returning [`ChunkSyncError::Timeout`]
    /// if read access could not be obtained within the given duration.
    /// - [`LockStrategy::Immediate`] will try to get the lock immediately (without blocking),
    /// returning [`ChunkSyncError::ImmediateFailure`] if it couldn't be done.
    /// - [`LockStrategy::Blocking`] will block indefinitely while waiting for the lock.
    ///
    /// [lock strategy]: LockStrategy
    pub fn read_handle(
        &self,
        strategy: LockStrategy,
    ) -> Result<ChunkReadHandle<'_>, ChunkSyncError> {
        match strategy {
            LockStrategy::Immediate => Ok(ChunkReadHandle {
                blocks: self
                    .blocks
                    .try_read()
                    .ok_or(ChunkSyncError::ImmediateFailure)?,
            }),
            LockStrategy::Blocking => Ok(ChunkReadHandle {
                blocks: self.blocks.read(),
            }),
            LockStrategy::Timeout(dur) => Ok(ChunkReadHandle {
                blocks: self
                    .blocks
                    .try_read_for(dur)
                    .ok_or(ChunkSyncError::Timeout(dur))?,
            }),
        }
    }

    /// Get a write handle for this chunk with the given [lock strategy].
    ///
    /// The returned error depends on the lock strategy:
    /// - [`LockStrategy::Timeout`] will block and wait for the lock, returning [`ChunkSyncError::Timeout`]
    /// if write access could not be obtained within the given duration.
    /// - [`LockStrategy::Immediate`] will try to get the lock immediately (without blocking),
    /// returning [`ChunkSyncError::ImmediateFailure`] if it couldn't be done.
    /// - [`LockStrategy::Blocking`] will block indefinitely while waiting for the lock.
    ///
    /// [lock strategy]: LockStrategy
    pub fn write_handle(
        &self,
        strategy: LockStrategy,
    ) -> Result<ChunkWriteHandle<'_>, ChunkSyncError> {
        match strategy {
            LockStrategy::Immediate => Ok(ChunkWriteHandle {
                flags: self
                    .flags
                    .try_write()
                    .ok_or(ChunkSyncError::ImmediateFailure)?,
                blocks: self
                    .blocks
                    .try_write()
                    .ok_or(ChunkSyncError::ImmediateFailure)?,
            }),
            LockStrategy::Blocking => Ok(ChunkWriteHandle {
                flags: self.flags.write(),
                blocks: self.blocks.write(),
            }),
            LockStrategy::Timeout(dur) => Ok(ChunkWriteHandle {
                flags: self
                    .flags
                    .try_write_for(dur)
                    .ok_or(ChunkSyncError::Timeout(dur))?,
                blocks: self
                    .blocks
                    .try_write_for(dur)
                    .ok_or(ChunkSyncError::Timeout(dur))?,
            }),
        }
    }

    /// Get the flags for this chunk, locking according to the given lock strategy.
    pub fn flags(&self, strategy: LockStrategy) -> Result<ChunkFlags, ChunkSyncError> {
        match strategy {
            LockStrategy::Immediate => Ok(self
                .flags
                .try_read()
                .ok_or(ChunkSyncError::ImmediateFailure)?
                .deref()
                .clone()),
            LockStrategy::Blocking => Ok(self.flags.read().clone()),
            LockStrategy::Timeout(dur) => Ok(self
                .flags
                .try_read_for(dur)
                .ok_or(ChunkSyncError::Timeout(dur))?
                .deref()
                .clone()),
        }
    }

    /// Set the flags of this chunk. You should usually always prefer [`Chunk::update_flags`] over
    /// this function as this function completely overwrites the existing flags.
    pub fn set_flags(
        &self,
        strategy: LockStrategy,
        new_flags: ChunkFlags,
    ) -> Result<(), ChunkSyncError> {
        let mut old_flags = match strategy {
            LockStrategy::Timeout(dur) => self
                .flags
                .try_write_for(dur)
                .ok_or(ChunkSyncError::Timeout(dur))?,
            LockStrategy::Immediate => self
                .flags
                .try_write()
                .ok_or(ChunkSyncError::ImmediateFailure)?,
            LockStrategy::Blocking => self.flags.write(),
        };

        *old_flags = new_flags;
        Ok(())
    }

    /// Calls the closure with a mutable reference to the existing flags, allowing the caller
    /// to make changes to specific flags while leaving others untouched.
    pub fn update_flags<F>(&self, strategy: LockStrategy, f: F) -> Result<(), ChunkSyncError>
    where
        F: for<'flags> FnOnce(&'flags mut ChunkFlags),
    {
        let old_flags = self.flags(strategy)?;
        let mut new_flags = old_flags;
        f(&mut new_flags);

        self.set_flags(strategy, new_flags)?;
        Ok(())
    }

    pub fn cached_load_reasons(&self) -> LoadReasons {
        self.load_reasons.read().cached_reasons
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
