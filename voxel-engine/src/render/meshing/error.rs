use std::error::Error;

use crate::topo::{
    access::ReadAccess,
    chunk_ref::ChunkRefVxlReadAccess,
    error::{ChunkManagerError, NeighborAccessError},
};

use super::MesherOutput;

type ReadError = <ChunkRefVxlReadAccess<'static> as ReadAccess>::ReadErr;

#[derive(te::Error, Debug)]
pub enum ChunkMeshingError {
    #[error("Mesher error: '{0}'")]
    MesherError(#[from] MesherError<ReadError, ReadError>),
    #[error(transparent)]
    ChunkManagerError(#[from] ChunkManagerError),
}

#[derive(te::Error, Debug)]
pub enum MesherError<A: Error, Nb: Error> {
    #[error("Access returned an error during meshing: {0}")]
    AccessError(A),
    #[error("Neighbor access returned an error during meshing: {0}")]
    NeighborAccessError(NeighborAccessError<Nb>),
    #[error("Mesher ran into an internal error: '{0}'")]
    CustomError(Box<dyn Error + Send>),
}

impl<A: Error, Nb: Error> MesherError<A, Nb> {
    pub fn custom<E: Error + Send + 'static>(error: E) -> Self {
        Self::CustomError(Box::new(error))
    }
}

pub type MesherResult<A, Nb> = Result<MesherOutput, MesherError<A, Nb>>;
