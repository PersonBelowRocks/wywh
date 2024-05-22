#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkManagerError {
    #[error("Chunk not loaded")]
    Unloaded,
    #[error("Chunk doesn't exist")]
    DoesntExist,
    #[error("Tried to initialize already existing chunk")]
    AlreadyInitialized,
}

#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkFlagError {
    #[error("Unknown flag(s) in chunk flags: {0}")]
    UnknownFlags(u32),
}
