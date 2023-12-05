use bevy::math::Vec2;

use crate::util::FaceMap;

use self::rotations::{BlockModelFace, BlockModelFaceMap, BlockModelRotation};

use super::{
    texture::FaceTexture,
    tile::{Face, Transparency},
};

pub mod descriptor;
pub mod rotations;

#[derive(Clone)]
pub struct VoxelProperties {
    pub transparency: Transparency,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockModel {
    pub textures: BlockModelFaceMap<FaceTexture>,
}

impl BlockModel {
    pub fn filled(tex_pos: Vec2) -> Self {
        Self {
            textures: BlockModelFaceMap::filled(FaceTexture::new(tex_pos)),
        }
    }

    pub fn faces_for_rotation(&self, rotation: BlockModelRotation) -> FaceMap<FaceTexture> {
        let mut map = FaceMap::new();
        for face in BlockModelFace::FACES {
            if let Some(tex) = self.texture(face) {
                map.set(rotation.get_cardinal_face(face), tex);
            }
        }

        map
    }

    pub fn texture(&self, face: BlockModelFace) -> Option<FaceTexture> {
        self.textures.get(face).copied()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VoxelModel {
    Block(BlockModel),
}

impl VoxelModel {
    pub fn as_block_model(self) -> Option<BlockModel> {
        match self {
            Self::Block(model) => Some(model),
            _ => None,
        }
    }
}
