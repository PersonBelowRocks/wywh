use std::path::PathBuf;

use super::{resourcepath::ResourcePath, tile::Face};

#[derive(te::Error, Debug)]
#[error("TODO")]
pub struct TileDataConversionError;

#[derive(te::Error, Debug)]
pub enum TextureLoadingError {
    #[error("Error loading texture to registry. Path: '{0}'")]
    FileNotFound(PathBuf),
    #[error("Texture was either not square or not 2D")]
    InvalidTextureDimensions,
}

#[derive(te::Error, Debug)]
pub enum BlockVariantFileLoaderError {
    #[error("Error walking through provided directory: {0}")]
    DirectoryWalkError(#[from] walkdir::Error),
    #[error("File at path {0} has invalid name")]
    InvalidFileName(PathBuf),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

#[derive(Copy, Clone, te::Error, Debug)]
pub enum TextureAtlasesGetAssetError {
    #[error("Color texture atlas handle did not exist in world")]
    MissingColorHandle,
    #[error("Could not find color texture atlas in assets")]
    MissingColor,
    #[error("Normal texture atlas handle did not exist in world")]
    MissingNormalHandle,
    #[error("Could not find normal texture atlas in assets")]
    MissingNormal,
}

#[derive(Copy, Clone, te::Error, Debug, Default)]
#[error("Error parsing {}", stringify!(FaceTextureRotation))]
pub struct FaceTextureRotationParseError;

#[derive(Copy, Clone, te::Error, Debug, Default)]
#[error("Error parsing {}", stringify!(Face))]
pub struct FaceParseError;

#[derive(Clone, te::Error, Debug, PartialEq, dm::Constructor)]
#[error("Error parsing {0} as face texture descriptor")]
pub struct FaceTextureDescParseError(String);

#[derive(Clone, te::Error, Debug, PartialEq, dm::Constructor)]
#[error("Error parsing {0} block model face")]
pub struct BlockModelFaceParseError(String);

#[derive(te::Error, Clone, Debug, PartialEq)]
pub enum BlockModelCreationError {
    #[error("Texture {0} not found in the provided texture registry")]
    TextureNotFound(ResourcePath),
    #[error("Descriptor doesn't provide texture for face {0:?}")]
    MissingFace(Face),
}
