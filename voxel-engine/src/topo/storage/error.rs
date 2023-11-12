#[derive(Copy, Clone, Debug, te::Error)]
#[error("Value {0} is out of bounds")]
pub struct OutOfBounds(pub usize);
