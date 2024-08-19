use super::world::ChunkHandleError;

#[derive(te::Error, Debug, Clone)]
pub enum NeighborReadError {
    #[error("Attempted to read out of bounds position")]
    OutOfBounds,
    #[error("Underlying read handle error: {0}")]
    Internal(#[from] ChunkHandleError),
}
