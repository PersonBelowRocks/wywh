#[derive(te::Error, Debug, Clone, PartialEq, Eq)]
pub enum FromPathError {
    #[error("Path is not valid UTF-8")]
    InvalidUtf8,
    #[error("{0}")]
    FromStrError(#[from] FromStrError),
}

#[derive(te::Error, Debug, Clone, PartialEq, Eq)]
pub enum FromStrError {
    #[error("Element {0} was invalid when parsing string")]
    InvalidElement(usize),
}
