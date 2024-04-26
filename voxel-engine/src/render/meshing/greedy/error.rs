

use crate::{
    topo::{
        error::{ChunkAccessError, NeighborAccessError},
    },
};

#[derive(te::Error, Debug, Clone, PartialEq)]
pub enum CqsError {
    #[error(transparent)]
    NeighborAccessError(#[from] NeighborAccessError),
    #[error(transparent)]
    AccessError(ChunkAccessError),
    #[error("Position was out of bounds")]
    OutOfBounds,
    #[error(
        "Attempted to access a microblock in a subdivided block with an out-of-bounds position"
    )]
    SubdivBlockAccessOutOfBounds,
}
