use std::path::PathBuf;

#[derive(te::Error, Debug)]
#[error("TODO")]
pub struct TileDataConversionError;

#[derive(te::Error, Debug)]
pub enum TextureLoadingError {
    #[error("Error loading texture to registry. Path: {0}")]
    FileNotFound(PathBuf),
    #[error("Textures must be square and 2D")]
    InvalidTextureDimensions(PathBuf),
}
