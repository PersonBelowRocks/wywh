use bevy::math::Vec2;

use crate::util::FaceMap;

use super::{
    texture::FaceTexture,
    tile::{Face, Transparency},
};

#[derive(Clone)]
pub struct VoxelProperties {
    pub transparency: Transparency,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockModel {
    pub textures: FaceMap<FaceTexture>,
}

impl BlockModel {
    pub fn filled(tex_pos: Vec2) -> Self {
        Self {
            textures: FaceMap::filled(FaceTexture::new(tex_pos)),
        }
    }

    pub fn texture(&self, face: Face) -> FaceTexture {
        *self.textures.get(face).unwrap()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoxelModel {
    Block(BlockModel),
}
