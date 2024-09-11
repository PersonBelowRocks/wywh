use super::world::ChunkHandleError;

#[derive(te::Error, Debug, Clone)]
pub enum NeighborReadError {
    #[error("Attempted to read out of bounds position")]
    OutOfBounds,
    #[error("Underlying read handle error: {0}")]
    Internal(#[from] ChunkHandleError),
}

/// Error returned when working with positions of chunks neighboring another chunk.
#[derive(te::Error, Debug, Clone, PartialEq, Eq)]
#[error("Neighbor position was either outside of the 3x3x3 box, or was the center of the box.")]
pub struct InvalidNeighborPosition;
