#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkManagerError {
    #[error("Chunk not loaded")]
    Unloaded,
    #[error("Chunk is primordial")]
    Primordial,
    #[error("Chunk doesn't exist")]
    DoesntExist,
    #[error("Tried to initialize already existing chunk")]
    AlreadyInitialized,
    #[error("Could associate the entity with a chunk")]
    MissingEntity,
    #[error("Chunk position is out of bounds")]
    OutOfBounds,
}

#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkFlagError {
    #[error("Unknown flag(s) in chunk flags: {0}")]
    UnknownFlags(u32),
}
