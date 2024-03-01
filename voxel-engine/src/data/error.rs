use std::path::PathBuf;

use super::{
    resourcepath::{error::FromPathError, ResourcePath},
    tile::Face,
    voxel::rotations::BlockModelFace,
};

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
pub enum VariantFileLoaderError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),
    #[error("{0}")]
    ParseError(#[from] deser_hjson::Error),
    #[error("Requested variant was not found")]
    VariantNotFound,
    #[error("Invalid variant file name: '{0}'")]
    InvalidFileName(PathBuf),
    #[error("Error parsing file path to ResourcePath")]
    ResourcePathError(#[from] FromPathError),
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
#[error("Error parsing {0} as submodel face texture descriptor")]
pub struct SubmodelFaceTextureDescParseError(String);

#[derive(Clone, te::Error, Debug, PartialEq, dm::Constructor)]
#[error("Error parsing {0} block model face")]
pub struct BlockModelFaceParseError(String);

#[derive(te::Error, Clone, Debug, PartialEq)]
pub enum BlockModelCreationError {
    #[error("Texture {0} not found in the provided texture registry")]
    TextureNotFound(ResourcePath),
    #[error("Descriptor doesn't provide model face {0:?}")]
    MissingModelFace(BlockModelFace),
    #[error("Descriptor submodel for direction {0:?} is missing face {1:?}")]
    MissingDirectionFace(Face, Face),
}
