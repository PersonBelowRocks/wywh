use std::error::Error;

use crate::topo::error::NeighborsAccessError;

#[derive(te::Error, Debug, Clone)]
pub enum CqsError<E: Error, NbErr: Error> {
    #[error(transparent)]
    NeighborAccessError(#[from] NeighborsAccessError<NbErr>),
    #[error(transparent)]
    AccessError(E),
    #[error("Position was out of bounds")]
    OutOfBounds,
}
