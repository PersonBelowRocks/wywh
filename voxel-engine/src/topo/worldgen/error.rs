use std::error::Error;

#[derive(te::Error, Debug, PartialEq, Eq)]
pub enum GeneratorError<E: Error> {
    #[error("Provided access does not have the bounding box of a chunk")]
    AccessNotChunk,
    #[error("Error while writing to provided access: {0}")]
    AccessError(#[from] E),
}
