#[derive(Copy, Clone, Debug, te::Error)]
#[error("Index is out of bounds")]
pub struct OutOfBounds;
