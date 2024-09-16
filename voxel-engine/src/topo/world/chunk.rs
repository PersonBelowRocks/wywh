use bevy::math::ivec3;
use bevy::prelude::*;
use bitflags::bitflags;
use octo::SubdividedStorage;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::fmt;
use std::ops::{Deref, DerefMut};

use crate::data::registries::block::{BlockVariantId, BlockVariantRegistry};
use crate::data::registries::Registry;
use crate::data::voxel::rotations::BlockModelRotation;
use crate::topo::block::SubdividedBlock;
use crate::topo::bounding_box::BoundingBox;
use crate::topo::controller::{LoadReasons, LoadshareMap};
use crate::topo::CHUNK_FULL_BLOCK_DIMS;
use crate::util::sync::{LockStrategy, StrategicReadLock, StrategicWriteLock, StrategySyncError};

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

    /// The center of the chunk in worldspace.
    pub fn worldspace_center(self) -> Vec3 {
        const HALF: f32 = (Chunk::SIZE as f32) / 2.0;
        self.worldspace_min().as_vec3() + HALF
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
        /// Indicates that the chunk was updated and should be remeshed.
        /// When the engine remeshes the chunk this flag will be unset.
        const REMESH = 0b1 << 1;
        // TODO: have flags for each edge that was updated
        /// Indicates that the chunk's neighbors should be remeshed.
        const REMESH_NEIGHBORS = 0b1 << 2;
        /// Indicates that this chunk was just generated and has not been meshed before.
        const FRESHLY_GENERATED = 0b1 << 3;
        /// Indicates that a chunk has not been populated with the generator and is only really
        /// acting as a "dummy" until it's further processed by the engine.
        /// Chunks are not supposed to be primordial for long, primordial chunks are usually immediately
        /// queued for further processing by the engine to get them out of their primordial state.
        const PRIMORDIAL = 0b1 << 4;
        /// Indicates that a chunk is entirely composed of opaque blocks. Such chunks can be
        /// used as occluders for occluder-based HZB culling.
        ///
        /// This flag is a potentially false negative hint, i.e.
        /// a chunk can still be opaque despite NOT having this flag.
        const OPAQUE = 0b1 << 5;
        /// Indicates that a chunk is entirely composed of transparent blocks. Such chunks don't have to have
        /// their mesh built.
        ///
        /// This flag is a potentially false negative hint, i.e.
        /// a chunk can still be transparent despite NOT having this flag.
        const TRANSPARENT = 0b1 << 6;
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
            (Self::OPAQUE, "SOLID"),
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

/// Test if the value can be safely stored in a subdivided storage.
/// Values must not have their highest bit set.
#[inline]
pub const fn valid_chunk_data_value(data: u32) -> bool {
    const MASK: u32 = 0b1 << (u32::BITS - 1);
    (data & MASK) == 0
}

pub type ChunkDataStorage = octo::SubdividedStorage<CHUNK_SIZE, BLOCK_SUBDIVISIONS, u32>;

/// The actual voxel data of a chunk
#[derive(Clone)]
pub struct ChunkData {
    default_value: BlockVariantId,
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
    /// # Panics
    /// Will panic if the default block variant ID is not valid to store in chunk data.
    /// See [`valid_chunk_data_value()`] for more information.
    pub fn new(default: BlockVariantId) -> Self {
        assert!(valid_chunk_data_value(default.as_u32()));

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
    pub fn initialize(&mut self) -> bool {
        match self.storage {
            None => {
                self.storage = Some(SubdividedStorage::new(self.default_value.as_u32()));
                true
            }
            Some(_) => false,
        }
    }

    /// Inflate the underlying storage, preparing it for writing. Does nothing if the
    /// chunk data is not initialized.
    ///
    /// See [`octo::SubdividedStorage::inflate()`] for more information.
    #[inline]
    pub fn inflate(&mut self) {
        self.storage.as_mut().map(|storage| storage.inflate());
    }

    /// Deflate the underlying storage, shrinking its memory footprint. Does nothing if the
    /// chunk data is not initialized.
    ///
    /// See [`octo::SubdividedStorage::delfate()`] for more information.
    #[inline]
    pub fn deflate(&mut self, unique: Option<usize>) {
        self.storage.as_mut().map(|storage| storage.deflate(unique));
    }

    /// Whether this chunk data is initialized or not.
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.storage.is_some()
    }

    /// Get the value at the given full-block position, returning an error if the position
    /// is out of bounds or if the block at the position was subdivided.
    /// # Vectors
    /// [`ls_pos`] is in full-block localspace.
    #[inline]
    pub fn get(&self, ls_pos: IVec3) -> Result<BlockVariantId, ChunkDataError> {
        if !Self::contains_full_block(ls_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        self.storage
            .as_ref()
            .map_or(Ok(self.default_value), |storage| {
                let index = ls_pos.to_array().map(|v| v as u8);
                storage
                    .get(index)
                    .map_err(ChunkDataError::from)
                    .map(|value| {
                        // We'll do some sanity checking in debug mode, but otherwise
                        // we don't validate the data when reading.
                        debug_assert!(valid_chunk_data_value(value));
                        // SAFETY: when writing to chunk data we check that the values written
                        // are valid
                        unsafe { BlockVariantId::from_raw(value) }
                    })
            })
    }

    /// Set the value at the given full-block localspace position.
    ///
    /// Returns an error in the following conditions:
    /// - The position is out of bounds.
    /// - The value is invalid to store in chunk data.
    ///
    /// If this is the first time writing to this chunk and the written value is not the default value,
    /// it will be initialized, which can cause memory allocation.
    /// # Vectors
    /// [`ls_pos`] is in full-block localspace.
    #[inline]
    pub fn set(&mut self, ls_pos: IVec3, value: BlockVariantId) -> Result<(), ChunkDataError> {
        if !Self::contains_full_block(ls_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        // Writing is pointless if the value is the default and we're not initialized.
        if value == self.default_value && !self.is_initialized() {
            return Ok(());
        }

        let raw_value = value.as_u32();
        // Check if the high bit is set in the u32 representation of the variant ID.
        if !valid_chunk_data_value(raw_value) {
            return Err(ChunkDataError::InvalidValue(raw_value));
        }

        let storage = self
            .storage
            .get_or_insert_with(|| ChunkDataStorage::new(self.default_value.as_u32()));

        let index = ls_pos.to_array().map(|v| v as u8);
        Ok(storage.set(index, raw_value)?)
    }

    /// Get the value at the given microblock position, returning an error if the position
    /// is out of bounds.
    /// # Vectors
    /// [`mb_pos`] is in microblock localspace.
    #[inline]
    pub fn get_mb(&self, mb_pos: IVec3) -> Result<BlockVariantId, ChunkDataError> {
        if !Self::contains_microblock(mb_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        self.storage
            .as_ref()
            .map_or(Ok(self.default_value), |storage| {
                let index = mb_pos.to_array().map(|v| v as u8);
                storage
                    .get_mb(index)
                    .map_err(ChunkDataError::from)
                    .map(|value| {
                        // We'll do some sanity checking in debug mode, but otherwise
                        // we don't validate the data when reading.
                        debug_assert!(valid_chunk_data_value(value));
                        // SAFETY: when writing to chunk data we check that the values written
                        // are valid
                        unsafe { BlockVariantId::from_raw(value) }
                    })
            })
    }

    /// Set the value at the given microblock localspace position.
    ///
    /// Returns an error in the following conditions:
    /// - The position is out of bounds.
    /// - The value is invalid to store in chunk data.
    ///
    /// If this is the first time writing to this chunk and the written value is not the default value,
    /// it will be initialized, which can cause memory allocation.
    /// ### Microblock allocations
    /// Writing microblocks can cause allocations in another way too, if a microblock is written
    /// to a position that currently contains a full-block, then that full-block has to be
    /// subdivided and moved into the subdivided block buffer, potentially allocating.
    /// # Vectors
    /// [`mb_pos`] is in microblock localspace.
    #[inline]
    pub fn set_mb(&mut self, mb_pos: IVec3, value: BlockVariantId) -> Result<(), ChunkDataError> {
        if !Self::contains_microblock(mb_pos) {
            return Err(ChunkDataError::OutOfBounds);
        }

        // Writing is pointless if the value is the default.
        if value == self.default_value && !self.is_initialized() {
            return Ok(());
        }

        let raw_value = value.as_u32();
        // Check if the high bit is set in the u32 representation of the variant ID.
        if !valid_chunk_data_value(raw_value) {
            return Err(ChunkDataError::InvalidValue(raw_value));
        }

        let storage = self
            .storage
            .get_or_insert_with(|| ChunkDataStorage::new(self.default_value.as_u32()));

        let index = mb_pos.to_array().map(|v| v as u8);
        Ok(storage.set_mb(index, raw_value)?)
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

    /// Tests if all blocks in this chunk are full-blocks and that they all pass some test.
    /// - Will return `true` if all blocks are full and pass the test.
    /// - Will return `false` if any block is not a full-block or if any of the blocks failed the test.
    ///
    /// This function should be ran on freshly deflated data otherwise it might produce invalid
    /// results (for example if there's a subdivided block but all its microblocks are the same).
    #[inline]
    pub fn all_full_blocks_and<F>(&self, test: F) -> bool
    where
        F: Fn(BlockVariantId) -> bool,
    {
        const SIZE: i32 = CHUNK_FULL_BLOCK_DIMS as i32;

        // If we're not initialized, all reads will return the default value, so instead of doing all the
        // reads we can just run the function once for our default value.
        if self.storage.is_none() {
            return test(self.default_value);
        }

        for x in 0..SIZE {
            for y in 0..SIZE {
                for z in 0..SIZE {
                    let ls_pos = ivec3(x, y, z);
                    match self.get(ls_pos) {
                        Ok(id) => {
                            if !test(id) {
                                return false;
                            }
                        }
                        // Not a full-block.
                        Err(ChunkDataError::NonFullBlock) => return false,
                        Err(_) => unreachable!(),
                    }
                }
            }
        }

        true
    }
}

macro_rules! impl_chunk_handle_reads {
    ($lt:lifetime, $name:ty) => {
        impl<$lt> $name {
            /// Get the block at the given full-block position.
            /// Returns [`None`] if the block at the given position is not a full-block.
            /// Errors if the position is out of bounds.
            /// # Vectors
            /// [`ls_pos`] is in full-block localspace.
            #[inline]
            pub fn get(&self, ls_pos: IVec3) -> Result<Option<BlockVariantId>, ChunkHandleError> {
                let variant_id = match self.blocks.get(ls_pos) {
                    Ok(id) => id,
                    Err(ChunkDataError::OutOfBounds) => {
                        return Err(ChunkHandleError::FullBlockOutOfBounds(ls_pos))
                    }
                    Err(ChunkDataError::NonFullBlock) => return Ok(None),
                    Err(ChunkDataError::InvalidValue(_)) => {
                        unreachable!("this error isn't returned when reading")
                    }
                };

                Ok(Some(variant_id))
            }

            /// Get the variant ID at the given microblock position.
            /// Errors if the position is out of bounds.
            /// # Vectors
            /// [`mb_pos`] is in microblock localspace.
            #[inline]
            pub fn get_mb(&self, mb_pos: IVec3) -> Result<BlockVariantId, ChunkHandleError> {
                let variant_id = match self.blocks.get_mb(mb_pos) {
                    Ok(id) => id,
                    Err(ChunkDataError::OutOfBounds) => {
                        return Err(ChunkHandleError::MicroblockOutOfBounds(mb_pos))
                    }
                    Err(ChunkDataError::NonFullBlock) => unreachable!(
                        "reading microblocks from a subdivided block is okay and intended"
                    ),
                    Err(ChunkDataError::InvalidValue(_)) => {
                        unreachable!("this error isn't returned when reading")
                    }
                };

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
        self.blocks.set(ls_pos, id).map_err(|err| match err {
            ChunkDataError::NonFullBlock => unreachable!(
                "it should not be possible to encounter a non-full-block error when writing"
            ),
            ChunkDataError::OutOfBounds => ChunkHandleError::FullBlockOutOfBounds(ls_pos),
            ChunkDataError::InvalidValue(value) => ChunkHandleError::InvalidDataValue(value),
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
        self.blocks.set_mb(mb_pos, id).map_err(|err| match err {
            ChunkDataError::NonFullBlock => unreachable!(
                "it should not be possible to encounter a non-full-block error when writing"
            ),
            ChunkDataError::OutOfBounds => ChunkHandleError::MicroblockOutOfBounds(mb_pos),
            ChunkDataError::InvalidValue(value) => ChunkHandleError::InvalidDataValue(value),
        })
    }

    /// Initializes the underlying data for writing. See [`ChunkData::touch`].
    #[inline]
    pub fn touch(&mut self) -> bool {
        // We inflate before initializing since the storage is inflated by default when initialized.
        self.blocks.inflate();
        self.blocks.initialize()
    }

    /// Compresses the chunk data in memory.
    ///
    /// See [`octo::SubdividedStorage::deflate()`] for more information.
    #[inline]
    pub fn deflate(&mut self, unique: Option<usize>) {
        self.blocks.deflate(unique);
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
    pub fn new(chunk_pos: ChunkPos, filling: BlockVariantId, initial_flags: ChunkFlags) -> Self {
        Self {
            chunk_pos,
            flags: RwLock::new(initial_flags),
            blocks: RwLock::new(ChunkData::new(filling)),
        }
    }

    #[inline]
    pub fn chunk_pos(&self) -> ChunkPos {
        self.chunk_pos
    }

    /// Get a read handle for this chunk with the given [lock strategy].
    /// The returned error depends on the lock strategy, see [`StrategySyncError`] for more information.
    ///
    /// [lock strategy]: LockStrategy
    pub fn read_handle(
        &self,
        strategy: LockStrategy,
    ) -> Result<ChunkReadHandle<'_>, StrategySyncError> {
        Ok(ChunkReadHandle {
            blocks: self.blocks.strategic_read(strategy)?,
        })
    }

    /// Get a write handle for this chunk with the given [lock strategy].
    /// The returned error depends on the lock strategy, see [`StrategySyncError`] for more information.
    ///
    /// [lock strategy]: LockStrategy
    pub fn write_handle(
        &self,
        strategy: LockStrategy,
    ) -> Result<ChunkWriteHandle<'_>, StrategySyncError> {
        Ok(ChunkWriteHandle {
            blocks: self.blocks.strategic_write(strategy)?,
        })
    }
}

#[cfg(test)]
mod test {
    use bevy::math::vec3;

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
