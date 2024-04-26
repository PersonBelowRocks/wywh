use std::error::Error;

use crate::topo::world::ChunkManagerError;

use super::{greedy::error::CqsError, MesherOutput};

#[derive(te::Error, Debug)]
pub enum ChunkMeshingError {
    #[error("Mesher error: '{0}'")]
    MesherError(#[from] MesherError),
    #[error(transparent)]
    ChunkManagerError(#[from] ChunkManagerError),
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

pub type MesherResult = Result<MesherOutput, MesherError>;
