use std::any::type_name;

use crate::util::FaceMap;

use super::{
    registry::{TextureId, VoxelTextureRegistry},
    tile::Transparency,
};

pub trait Voxel: Default {
    fn label() -> &'static str {
        type_name::<Self>()
    }

    fn model(textures: &VoxelTextureRegistry) -> Option<VoxelModel>;

    fn properties() -> VoxelProperties;
}

#[derive(Clone)]
pub struct VoxelProperties {
    pub transparency: Transparency,
}

#[derive(Copy, Clone)]
pub struct BlockModel {
    pub textures: FaceMap<TextureId>,
}

impl BlockModel {
    pub fn filled(id: TextureId) -> Self {
        Self {
            textures: FaceMap::filled(id),
        }
    }
}

#[derive(Copy, Clone)]
pub enum VoxelModel {
    Block(BlockModel),
}
