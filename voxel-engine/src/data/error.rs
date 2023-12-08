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
pub struct FaceTextureDescriptorParseError(String);

impl From<FaceTextureRotationParseError> for FaceTextureDescriptorParseError {
    fn from(value: FaceTextureRotationParseError) -> Self {
        Self(value.0)
    }
}

#[derive(te::Error, Debug)]
pub enum BlockModelDescriptorParseError {}

#[derive(te::Error, Debug, dm::Constructor)]
#[error("Couldn't parse '{0}' into a face texture rotation")]
pub struct FaceTextureRotationParseError(String);

#[derive(te::Error, Debug, dm::Constructor)]
#[error("Couldn't parse '{0}' into a block model face")]
pub struct BlockModelFaceParseError(String);

#[derive(te::Error, Debug, dm::Constructor)]
#[error("Couldn't parse '{0}' into a face")]
pub struct FaceParseError(String);

#[derive(te::Error, Debug)]
pub enum RotatedTextureDescriptorParseError {
    #[error("{0}")]
    FaceTextureRotation(#[from] FaceTextureRotationParseError),
    #[error("{0}")]
    BlockModelFace(#[from] BlockModelFaceParseError),
    #[error("{0}")]
    Face(#[from] FaceParseError),
}
