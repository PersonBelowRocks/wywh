use crate::{render::occlusion::BlockOcclusion, util::FaceMap};

use self::{
    descriptor::{BlockVariantDescriptor},
    rotations::{BlockModelFace, BlockModelFaceMap, BlockModelRotation},
};

use super::{
    registries::{texture::TextureRegistry},
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
        _descriptor: &BlockVariantDescriptor,
        _registry: &TextureRegistry,
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
pub struct BlockSubmodel(FaceMap<SubmodelFaceTexture>);

impl BlockSubmodel {
    fn selfref_no_tex_rot_submodel(model_rotation: BlockModelRotation) -> BlockSubmodel {
        let mut map = FaceMap::<SubmodelFaceTexture>::new();

        for model_face in BlockModelFace::FACES {
            let world_face = model_rotation.get_cardinal_face(model_face);
            map.set(
                world_face,
                SubmodelFaceTexture::SelfFace {
                    face: model_face,
                    rotation: Default::default(),
                },
            );
        }

        BlockSubmodel::from_map(map).unwrap()
    }

    pub fn from_map(map: FaceMap<SubmodelFaceTexture>) -> Option<Self> {
        if map.is_filled() {
            Some(Self(map))
        } else {
            None
        }
    }
}

impl BlockModel {
    pub fn submodel(&self, direction: Face) -> SubmodelRef<'_> {
        let submodel =
            *self
                .directions
                .get(direction)
                .unwrap_or(&BlockSubmodel::selfref_no_tex_rot_submodel(
                    BlockModelRotation::DEFAULT,
                ));

        SubmodelRef {
            parent: &self,
            model: submodel,
        }
    }

    pub fn default_submodel(&self) -> SubmodelRef<'_> {
        SubmodelRef {
            parent: &self,
            model: BlockSubmodel::selfref_no_tex_rot_submodel(BlockModelRotation::DEFAULT),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SubmodelRef<'a> {
    parent: &'a BlockModel,
    model: BlockSubmodel,
}

impl<'a> SubmodelRef<'a> {
    pub fn texture(&self, face: Face) -> FaceTexture {
        match *self.model.0.get(face).unwrap() {
            SubmodelFaceTexture::Unique(tex) => tex,
            SubmodelFaceTexture::SelfFace { face, rotation } => {
                let mut tex = *self.parent.model.get(face).unwrap();
                tex.rotation += rotation;

                tex
            }
        }
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
