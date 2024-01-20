use std::error::Error;

use crate::topo::error::NeighborAccessError;

#[derive(te::Error, Debug, Clone)]
pub enum CqsError<E: Error, NbErr: Error> {
    #[error(transparent)]
    NeighborAccessError(#[from] NeighborAccessError<NbErr>),
    #[error(transparent)]
    AccessError(E),
    #[error("Position was out of bounds")]
    OutOfBounds,
}
