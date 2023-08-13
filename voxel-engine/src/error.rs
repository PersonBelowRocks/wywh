use crate::util::ConversionError;

#[derive(te::Error, Debug)]
pub enum ChunkVoxelError {
    /// Could not convert the position vector's components into [`usize`]. (Usually [`i32`] -> [`usize`])
    #[error("{0}")]
    ConversionError(#[from] ConversionError),
    /// The position is out of bounds.
    #[error("Position out of bounds")]
    OutOfBounds,
    /// The voxel storage is not initialized.
    #[error("Voxel storage not initialized")]
    NotInitializedError,
}

#[derive(te::Error, Debug)]
#[error("TODO")]
pub struct TileDataConversionError;
