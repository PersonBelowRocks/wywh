use bevy::sprite::TextureAtlasBuilderError;

#[derive(Debug, te::Error)]
pub enum TextureRegistryError {
    #[error("Error while building texture atlas: {0}")]
    AtlasBuilderError(#[from] TextureAtlasBuilderError),
}

#[derive(Debug, te::Error)]
pub enum VariantRegistryError {
    #[error("{0}")]
    TextureNotFound(#[from] TextureNotFound),
}

#[derive(Debug, te::Error)]
#[error("Texture with label '{0}' not found")]
pub struct TextureNotFound(pub String);
