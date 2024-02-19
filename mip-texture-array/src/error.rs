use bevy::prelude::*;

#[derive(te::Error, Debug, Clone)]
pub enum TextureArrayBuilderError {
    #[error("Wrong image dimensions, expected square image with dimensions {ed}x{ed}, got image with dimensions {x}x{y}.")]
    IncorrectImageDimensions { ed: u32, x: u32, y: u32 },
    #[error("Image not found in provided assets. Image handle: {0:?}")]
    ImageNotFound(AssetId<Image>),
}
