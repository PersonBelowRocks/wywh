use std::path::PathBuf;

#[derive(te::Error, Debug)]
#[error("TODO")]
pub struct TileDataConversionError;

#[derive(te::Error, Debug)]
pub enum TextureLoadingError {
    #[error("Error loading texture to registry. Path: {0}")]
    FileNotFound(PathBuf),
    #[error("Texture was either not square or not 2D")]
    InvalidTextureDimensions,
}

#[derive(te::Error, Debug)]
pub enum VariantFileLoaderError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),
    #[error("{0}")]
    ParseError(#[from] deser_hjson::Error),
    #[error("Variant not found: '{0}'")]
    VariantNotFound(String),
    #[error("Invalid variant file name: {0}")]
    InvalidFileName(PathBuf),
}

#[derive(te::Error, Debug, dm::Constructor)]
#[error("Couldn't parse '{0}' into a rotated texture descriptor")]
pub struct RotatedTextureDescriptorParseError(String);
