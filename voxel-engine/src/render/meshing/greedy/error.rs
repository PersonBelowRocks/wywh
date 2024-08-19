use crate::topo::{error::NeighborReadError, world::ChunkHandleError};

#[derive(te::Error, Debug, Clone)]
pub enum CqsError {
    #[error(transparent)]
    NeighborAccessError(#[from] NeighborReadError),
    #[error(transparent)]
    HandleError(#[from] ChunkHandleError),
    #[error("Position was out of bounds")]
    OutOfBounds,
    #[error(
        "Attempted to access a microblock in a subdivided block with an out-of-bounds position"
    )]
    SubdivBlockAccessOutOfBounds,
}
