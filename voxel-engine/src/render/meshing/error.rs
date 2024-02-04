use crate::{
    render::error::MesherError,
    topo::{access::ReadAccess, chunk_ref::ChunkRefVxlReadAccess, error::ChunkManagerError},
};

type ReadError = <ChunkRefVxlReadAccess<'static> as ReadAccess>::ReadErr;

#[derive(te::Error, Debug)]
pub enum ChunkMeshingError {
    #[error("Mesher error: '{0}'")]
    MesherError(#[from] MesherError<ReadError, ReadError>),
    #[error(transparent)]
    ChunkManagerError(#[from] ChunkManagerError),
}
