#[derive(Copy, Clone, Debug, PartialEq, Eq, te::Error)]
#[error("Position is out of bounds")]
pub struct OutOfBounds;

// #[derive(Copy, Clone, Debug, PartialEq, Eq, te::Error)]
// pub enum IndexedChunkStorage
