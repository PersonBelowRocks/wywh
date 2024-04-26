use std::error::Error;

use crate::util::ConversionError;

use super::storage::error::OutOfBounds;

#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkAccessError {
    /// Could not convert the position vector's components into [`usize`]. (Usually [`i32`] -> [`usize`])
    #[error("{0}")]
    ConversionError(#[from] ConversionError),
    /// The position is out of bounds.
    #[error("Position out of bounds")]
    OutOfBounds,
    /// The voxel storage is not initialized.
    #[error("Voxel storage not initialized")]
    NotInitialized,
}

impl From<OutOfBounds> for ChunkAccessError {
    fn from(_value: OutOfBounds) -> Self {
        Self::OutOfBounds
    }
}

#[derive(te::Error, Debug, PartialEq, Eq)]
pub enum ChunkManagerError {
    #[error("Chunk not loaded")]
    Unloaded,
    #[error("Chunk doesn't exist")]
    DoesntExist,
    #[error("Tried to initialize already existing chunk")]
    AlreadyInitialized,
}

#[derive(te::Error, Debug, PartialEq, Eq)]
pub enum ChunkRefAccessError {
    #[error("Chunk has been unloaded")]
    Unloaded,
}

#[derive(te::Error, Debug, Clone, PartialEq)]
pub enum NeighborAccessError {
    #[error("Attempted to access out of bounds position")]
    OutOfBounds,
    #[error("Underlying access error: {0}")]
    Internal(#[from] ChunkAccessError),
}
