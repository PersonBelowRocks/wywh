use std::time::Duration;

use bevy::math::IVec3;

use crate::util::sync::StrategySyncError;

#[derive(te::Error, Debug, Clone)]
pub enum ChunkManagerError {
    #[error("Chunk not loaded")]
    Unloaded,
    #[error(transparent)]
    ContainerError(#[from] ChunkContainerError),
    #[error(transparent)]
    SyncError(#[from] StrategySyncError),
    #[error("Chunk is primordial")]
    Primordial,
    #[error("Tried to initialize already existing chunk")]
    AlreadyLoaded,
    #[error("Could associate the entity with a chunk")]
    MissingEntity,
    #[error("Chunk position is out of bounds")]
    OutOfBounds,
}

impl ChunkManagerError {
    pub fn is_globally_locked(&self) -> bool {
        matches!(
            self,
            Self::ContainerError(ChunkContainerError::GloballyLocked)
        )
    }

    pub fn is_doesnt_exists(&self) -> bool {
        matches!(self, Self::ContainerError(ChunkContainerError::DoesntExist))
    }
}

#[derive(te::Error, Debug, Clone)]
pub enum ChunkContainerError {
    #[error("Chunk doesn't exist")]
    DoesntExist,
    #[error("Chunk container is globally locked")]
    GloballyLocked,
    #[error("Chunk is not loaded under this loadshare")]
    InvalidLoadshare,
}

/// Errors related to low-level chunk data reads and writes.
#[derive(te::Error, Debug, Clone)]
pub enum ChunkDataError {
    /// The provided position for a chunk data operation (either read or write) is out of bounds
    #[error("Out of bounds")]
    OutOfBounds,
    /// Attempted to read a full block at a position that had a subdivided block
    #[error("Not a full block")]
    NonFullBlock,
    #[error("Value {0:#01x?} cannot be stored in a subdivided storage")]
    InvalidValue(u32),
}

impl From<octo::SubdivAccessError> for ChunkDataError {
    fn from(value: octo::SubdivAccessError) -> Self {
        use octo::SubdivAccessError as Error;

        match value {
            Error::OutOfBounds(_) => Self::OutOfBounds,
            Error::NonFullBlock(_, _) => Self::NonFullBlock,
        }
    }
}

/// Errors related to chunk handle operations. Closely related to [`ChunkDataError`].
#[derive(te::Error, Debug, Clone)]
pub enum ChunkHandleError {
    /// Microblock position is out of bounds
    #[error("Microblock position {0} is out of bounds for the chunk handle")]
    MicroblockOutOfBounds(IVec3),
    /// Full-block position is out of bounds
    #[error("Full-block position {0} is out of bounds for the chunk handle")]
    FullBlockOutOfBounds(IVec3),
    /// The value stored in the chunk data is not a valid block variant ID
    #[error("Can't create block variant ID from raw value: {0:#01x}")]
    InvalidDataValue(u32),
}

#[derive(te::Error, Debug, Clone)]
pub enum ChunkFlagError {
    #[error("Unknown flag(s) in chunk flags: {0}")]
    UnknownFlags(u32),
}

#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
#[error("Index was out of bounds for volume")]
pub struct OutOfBounds;
