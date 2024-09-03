use bevy::prelude::Deref;

use crate::{data::error, topo::world::ChunkPos};

/// Implement an `out_of_bounds` function for this type to easily create an out-of-bounds error from
/// a chunk position. The target type must be an enum with a variant called `OutOfBounds` with a field of
/// [`ChunkPosOutOfBounds`].
macro_rules! impl_oob_error {
    ($t:ty) => {
        impl $t {
            /// Create an out-of-bounds error.
            /// Returned value will be a [`ChunkLoadError::OutOfBounds`] variant.
            #[inline]
            pub fn out_of_bounds(chunk_pos: ChunkPos) -> Self {
                Self::OutOfBounds(ChunkPosOutOfBounds(chunk_pos))
            }
        }
    };
}

/// Error produced when loading a chunk.
#[derive(te::Error, Debug, Clone)]
pub enum ChunkLoadError {
    /// There was already a chunk loaded for this position.
    /// This error does not need much handling, it's mainly a warning to the caller
    /// that their operation had no effect.
    #[error("Chunk {0} is already loaded")]
    AlreadyLoaded(ChunkPos),
    /// The chunk position is out of bounds for the world.
    #[error(transparent)]
    OutOfBounds(#[from] ChunkPosOutOfBounds),
    /// Tried to load chunk without any load reasons.
    /// Chunks without load reasons should always be in purgatory.
    #[error("Cannot load chunk {0} with no load reasons")]
    NoReasons(ChunkPos),
}

impl_oob_error!(ChunkLoadError);

/// Error produced when purging a chunk.
#[derive(te::Error, Debug, Clone)]
pub enum ChunkPurgeError {
    /// No chunk was loaded at this position.
    /// Much like [`ChunkLoadError::AlreadyLoaded`] this error does not need much
    /// handling, since attempting to purge a not-loaded chunk is usually just a no-op.
    #[error("Chunk {0} is not loaded")]
    NotLoaded(ChunkPos),
    #[error("Chunk {0} is already in purgatory.")]
    AlreadyPurged(ChunkPos),
    /// The chunk position is out of bounds for the world.
    #[error(transparent)]
    OutOfBounds(#[from] ChunkPosOutOfBounds),
}

impl_oob_error!(ChunkPurgeError);

/// Error produced when getting a loaded chunk.
#[derive(te::Error, Debug, Clone)]
pub enum ChunkGetError {
    /// This chunk was not loaded so no reference to it could be created.
    #[error("Chunk {0} is not loaded")]
    NotLoaded(ChunkPos),
    /// The chunk position is out of bounds for the world.
    #[error(transparent)]
    OutOfBounds(#[from] ChunkPosOutOfBounds),
}

impl_oob_error!(ChunkGetError);

#[derive(te::Error, Debug, Clone)]
pub enum CmStructuralError {
    /// This chunk is not loaded, so the operation cannot complete.
    #[error("Chunk is not loaded")]
    NotLoaded,
    /// This chunk is loaded, but not in the loadshare, so the operation cannot complete.
    #[error("Chunk is not in this loadshare")]
    NotInLoadshare,
    /// Attempted to load a chunk with no load reasons
    #[error("No load reasons")]
    NoLoadReasons,
    /// This chunk is already loaded and cannot be loaded twice.
    #[error("Chunk is already loaded")]
    ChunkAlreadyLoaded,
}

/// General out of bounds error for chunks.
/// See the `WORLD_HORIZONTAL_DIMENSIONS` and `WORLD_VERTICAL_DIMENSIONS` constants for more information.
#[derive(te::Error, Debug, Clone, Deref, dm::Into)]
#[error("Chunk position {0} is out of bounds")]
pub struct ChunkPosOutOfBounds(ChunkPos);
