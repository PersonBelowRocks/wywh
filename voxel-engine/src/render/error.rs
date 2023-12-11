use std::error::Error;

#[derive(te::Error, Debug, Clone)]
pub enum MesherError<AE: Error> {
    #[error("Access returned an error during meshing: {0}")]
    AccessError(#[from] AE),
    #[error("Mesher ran into an internal error: '{0}'")]
    InternalError(String),
}
