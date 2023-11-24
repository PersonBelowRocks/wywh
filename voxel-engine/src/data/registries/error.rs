use bevy::sprite::TextureAtlasBuilderError;

#[derive(Debug, te::Error)]
pub enum TextureRegistryError {
    #[error("Error while building texture atlas: {0}")]
    AtlasBuilderError(#[from] TextureAtlasBuilderError),
}
