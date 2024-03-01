use crate::{data::registries::Registry, render::occlusion::BlockOcclusion, util::FaceMap};

use self::{
    descriptor::{BlockVariantDescriptor, FaceTextureDescriptor},
    rotations::{BlockModelFace, BlockModelFaceMap, BlockModelRotation},
};

use super::{
    registries::{error::TextureNotFound, texture::TextureRegistry},
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
    pub directions: FaceMap<BlockSubmodel>,
    pub model: BlockModelFaceMap<FaceTexture>,
}

impl BlockModel {
    pub fn from_descriptor(
        descriptor: &BlockVariantDescriptor,
        registry: &TextureRegistry,
    ) -> Result<Self, ()> {
        todo!()
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum SubmodelFaceTexture {
    SelfFace {
        face: BlockModelFace,
        rotation: FaceTextureRotation,
    },
    Unique(FaceTexture),
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockSubmodel([SubmodelFaceTexture; 6]);

impl BlockSubmodel {
    pub(crate) fn from_arr(arr: [SubmodelFaceTexture; 6]) -> Self {
        Self(arr)
    }

    pub fn get_texture(&self, face: Face) -> FaceTexture {
        todo!()
    }
}

impl BlockModel {
    pub fn submodel(&self, direction: Face) -> &BlockSubmodel {
        todo!()
    }

    pub fn default_submodel(&self) -> &BlockSubmodel {
        todo!()
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

    pub fn occlusion(&self, _rotation: Option<BlockModelRotation>) -> BlockOcclusion {
        todo!()
    }
}
