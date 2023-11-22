#[derive(Clone, Debug, te::Error)]
pub enum RegistryError {
    #[error("Expected registry to be frozen")]
    RegistryNotFrozen,
    #[error("Expected registry to be loading")]
    RegistryNotLoading,
}
