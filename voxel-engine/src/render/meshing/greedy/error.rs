use std::error::Error;

use crate::{
    render::meshing::error::MesherError,
    topo::{
        error::{ChunkAccessError, NeighborAccessError},
        storage::error::OutOfBounds,
    },
};

#[derive(te::Error, Debug, Clone)]
pub enum CqsError {
    #[error(transparent)]
    NeighborAccessError(#[from] NeighborAccessError),
    #[error(transparent)]
    AccessError(ChunkAccessError),
    #[error("Position was out of bounds")]
    OutOfBounds,
}

impl From<CqsError> for MesherError {
    fn from(value: CqsError) -> Self {
        match value {
            CqsError::OutOfBounds => Self::custom(OutOfBounds),
            CqsError::AccessError(err) => Self::AccessError(err),
            CqsError::NeighborAccessError(err) => Self::NeighborAccessError(err),
        }
    }
}
