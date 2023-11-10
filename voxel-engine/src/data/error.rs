use std::path::PathBuf;

use super::tile::TextureId;

#[derive(te::Error, Debug)]
#[error("TODO")]
pub struct TileDataConversionError;

#[derive(te::Error, Debug)]
pub enum TextureLoadingError {
    #[error("Error loading texture to registry. Path: {0}")]
    FileNotFound(PathBuf),
    #[error("Texture {0:?} was either not square or not 2D")]
    InvalidTextureDimensions(TextureId),
}
