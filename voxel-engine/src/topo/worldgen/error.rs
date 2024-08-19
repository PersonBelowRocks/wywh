use std::error::Error;

#[derive(te::Error, Debug, PartialEq, Eq)]
pub enum GeneratorError<E: Error> {
    #[error("Error while writing to provided access: {0}")]
    HandleError(#[from] E),
}
