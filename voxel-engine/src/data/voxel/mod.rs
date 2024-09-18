use crate::util::FaceMap;

use self::{
    descriptor::BlockVariantDescriptor,
    rotations::{BlockModelFace, BlockModelFaceMap, BlockModelRotation},
};

use super::{
    registries::texture::TextureRegistry,
    texture::{FaceTexture, FaceTextureRotation},
    tile::{Face, Transparency},
};

pub mod descriptor;
pub mod rotations;
pub mod serialization;

#[derive(Clone)]
pub struct VoxelProperties {
    pub transparency: Transparency,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockModel {
    pub faces: FaceMap<FaceTexture>,
}

impl BlockModel {
    pub fn texture(&self, face: Face) -> FaceTexture {
        *self.faces.get(face).unwrap()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoxelModel {
    Block(BlockModel),
}

impl VoxelModel {
    pub fn into_block_model(self) -> Option<BlockModel> {
        match self {
            Self::Block(model) => Some(model),
            _ => None,
        }
    }
}
