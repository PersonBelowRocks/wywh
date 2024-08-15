#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkManagerError {
    #[error("Chunk not loaded")]
    Unloaded,
    #[error(transparent)]
    ContainerError(#[from] ChunkContainerError),
    #[error("Chunk is primordial")]
    Primordial,
    #[error("Tried to initialize already existing chunk")]
    AlreadyLoaded,
    #[error("Could associate the entity with a chunk")]
    MissingEntity,
    #[error("Chunk position is out of bounds")]
    OutOfBounds,
}

impl ChunkManagerError {
    pub fn is_globally_locked(&self) -> bool {
        matches!(
            self,
            Self::ContainerError(ChunkContainerError::GloballyLocked)
        )
    }

    pub fn is_doesnt_exists(&self) -> bool {
        matches!(self, Self::ContainerError(ChunkContainerError::DoesntExist))
    }
}

#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkContainerError {
    #[error("Chunk doesn't exist")]
    DoesntExist,
    #[error("Chunk container is globally locked")]
    GloballyLocked,
    #[error("Chunk is not loaded under this loadshare")]
    InvalidLoadshare,
}

#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
pub enum ChunkFlagError {
    #[error("Unknown flag(s) in chunk flags: {0}")]
    UnknownFlags(u32),
}

#[derive(te::Error, Debug, PartialEq, Eq, Clone)]
#[error("Index was out of bounds for volume")]
pub struct OutOfBounds;
