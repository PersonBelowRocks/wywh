use std::any::type_name;

use bevy::{
    asset::{AssetId, AssetPath, UntypedAssetIdConversionError, UntypedHandle},
    render::texture::Image,
};
use mip_texture_array::TextureArrayBuilderError;

use crate::data::{
    error::{BlockModelCreationError, BlockVariantFileLoaderError},
    resourcepath::{error::FromPathError, ResourcePath},
    systems::{VoxelNormalMapFolder, VoxelTextureFolder},
};

#[derive(te::Error, Debug)]
pub enum TextureRegistryError {
    #[error("Could not get path for handle '{0:?}'")]
    CannotMakePath(UntypedHandle),
    #[error("Bad file name for texture: '{0}'")]
    BadFileName(AssetPath<'static>),
    #[error("World does not contain resource '{}'", type_name::<VoxelTextureFolder>())]
    VoxelTextureFolderNotFound,
    #[error("Voxel texture folder asset is not loaded")]
    VoxelTextureFolderNotLoaded,
    #[error("World does not contain resource '{}'", type_name::<VoxelNormalMapFolder>())]
    VoxelNormalMapFolderNotFound,
    #[error("Voxel normal map folder asset is not loaded")]
    VoxelNormalMapFolderNotLoaded,
    #[error("Atlas builder error: {0}")]
    BuilderError(#[from] TextureArrayBuilderError),
    #[error("Unexpected asset ID type: {0}")]
    UnexpectedAssetIdType(#[from] UntypedAssetIdConversionError),
    #[error("'{0:?}' was not a square image")]
    InvalidImageDimensions(AssetPath<'static>),
    #[error("Texture does not exist: '{0:?}'")]
    TextureDoesntExist(AssetPath<'static>),
    #[error("Texture not loaded: '{0}'")]
    TextureNotLoaded(AssetId<Image>),
    #[error("Error constructing resource path: {0}")]
    ResourcePathError(#[from] FromPathError),
}

#[derive(Debug, te::Error)]
#[error("Texture with label '{0}' not found")]
pub struct TextureNotFound(pub ResourcePath);

#[derive(Debug, te::Error)]
pub enum BlockVariantRegistryLoadError {
    #[error("Error creating block model: {0}")]
    ModelCreationError(#[from] BlockModelCreationError),
    #[error("Error loading block variant from file: {0}")]
    FileLoadError(#[from] BlockVariantFileLoaderError),
    #[error("Error parsing block variant TOML file: {0}")]
    TomlParseError(#[from] toml::de::Error),
}
