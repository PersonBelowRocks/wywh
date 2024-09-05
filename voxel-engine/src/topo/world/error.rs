use bevy::math::IVec3;

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
