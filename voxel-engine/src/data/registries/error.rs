use std::any::type_name;

use bevy::{
    asset::{AssetId, AssetPath, UntypedAssetIdConversionError, UntypedHandle},
    render::texture::Image,
    sprite::TextureAtlasBuilderError,
};

use crate::data::{error::VoxelModelCreationError, systems::VoxelTextureFolder};

#[derive(te::Error, Debug)]
pub enum TextureRegistryError {
    #[error("Could not get path for handle {0:?}")]
    CannotMakePath(UntypedHandle),
    #[error("Bad file name for texture: '{0}'")]
    BadFileName(AssetPath<'static>),
    #[error("World does not contain resource '{}'", type_name::<VoxelTextureFolder>())]
    VoxelTextureFolderNotFound,
    #[error("Voxel texture folder asset is not loaded")]
    VoxelTextureFolderNotLoaded,
    #[error("Atlas builder error: {0}")]
    AtlasBuilderError(#[from] TextureAtlasBuilderError),
    #[error("Unexpected asset ID type: {0}")]
    UnexpectedAssetIdType(#[from] UntypedAssetIdConversionError),
    #[error("{0:?} was not a square image")]
    InvalidImageDimensions(AssetPath<'static>),
    #[error("Texture does not exist: {0:?}")]
    TextureDoesntExist(AssetPath<'static>),
    #[error("Texture not loaded: '{0}'")]
    TextureNotLoaded(AssetId<Image>),
}

#[derive(Debug, te::Error)]
pub enum VariantRegistryError {
    #[error("{0}")]
    VoxelModelCreationError(#[from] VoxelModelCreationError),
}

#[derive(Debug, te::Error)]
#[error("Texture with label '{0}' not found")]
pub struct TextureNotFound(pub String);
