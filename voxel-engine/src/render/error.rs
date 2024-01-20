use std::error::Error;

use crate::topo::error::NeighborAccessError;

use super::mesh_builder::MesherOutput;

#[derive(te::Error, Debug)]
pub enum MesherError<A: Error, Nb: Error> {
    #[error("Access returned an error during meshing: {0}")]
    AccessError(A),
    #[error("Neighbor access returned an error during meshing: {0}")]
    NeighborAccessError(NeighborAccessError<Nb>),
    #[error("Mesher ran into an internal error: '{0}'")]
    CustomError(Box<dyn Error>),
}

impl<A: Error, Nb: Error> MesherError<A, Nb> {
    pub fn custom<E: Error + 'static>(error: E) -> Self {
        Self::CustomError(Box::new(error))
    }
}

pub type MesherResult<A, Nb> = Result<MesherOutput, MesherError<A, Nb>>;
