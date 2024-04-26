#[derive(te::Error, Debug, PartialEq, Eq)]
pub enum ChunkManagerError {
    #[error("Chunk not loaded")]
    Unloaded,
    #[error("Chunk doesn't exist")]
    DoesntExist,
    #[error("Tried to initialize already existing chunk")]
    AlreadyInitialized,
}
