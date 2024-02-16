use std::error::Error;

use crate::{
    render::error::MesherError,
    topo::{error::NeighborAccessError, storage::error::OutOfBounds},
};

#[derive(te::Error, Debug, Clone)]
pub enum CqsError<E: Error, NbErr: Error> {
    #[error(transparent)]
    NeighborAccessError(#[from] NeighborAccessError<NbErr>),
    #[error(transparent)]
    AccessError(E),
    #[error("Position was out of bounds")]
    OutOfBounds,
}

impl<E, NbErr> From<CqsError<E, NbErr>> for MesherError<E, NbErr>
where
    E: Error,
    NbErr: Error,
{
    fn from(value: CqsError<E, NbErr>) -> Self {
        match value {
            CqsError::OutOfBounds => Self::custom(OutOfBounds),
            CqsError::AccessError(err) => Self::AccessError(err),
            CqsError::NeighborAccessError(err) => Self::NeighborAccessError(err),
        }
    }
}
