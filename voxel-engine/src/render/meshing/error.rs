use std::error::Error;

use crate::topo::world::chunk_manager::error::ChunkGetError;

use super::{controller::ChunkMeshData, greedy::error::CqsError};

/// Error when creating a mesh for a chunk.
#[derive(te::Error, Debug)]
pub enum ChunkMeshingError {
    /// Error produced and returned by the internal meshing algorithm.
    #[error("Mesher error: '{0}'")]
    MesherError(#[from] MesherError),
    /// Error when getting a chunk to mesh.
    #[error(transparent)]
    ChunkGetError(#[from] ChunkGetError),
    /// Attempted to mesh a primordial (aka. uninitialized) chunk. These chunks have no data and
    /// should only be read from after they've been initialized.
    #[error("Cannot mesh a primordial chunk")]
    PrimordialChunk,
}

#[derive(te::Error, Debug)]
pub enum MesherError {
    #[error("CQS error in mesher: {0}")]
    CqsError(#[from] CqsError),
    #[error("Mesher ran into an internal error: '{0}'")]
    CustomError(Box<dyn Error + Send>),
}

impl MesherError {
    pub fn custom<E: Error + Send + 'static>(error: E) -> Self {
        Self::CustomError(Box::new(error))
    }
}

pub type MesherResult = Result<ChunkMeshData, MesherError>;
