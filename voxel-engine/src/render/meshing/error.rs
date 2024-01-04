use std::error::Error;

use crate::topo::storage::error::OutOfBounds;

#[derive(te::Error, Debug, Clone)]
pub enum NeighborsAccessError<E: Error> {
    #[error("Attempted to access out of bounds position")]
    OutOfBounds,
    #[error("Underlying access error: {0}")]
    Internal(#[from] E),
}
