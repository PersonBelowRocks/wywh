use crate::{
    data::{error::SubmodelFromDescriptorError, registries::Registry},
    util::FaceMap,
};

use self::descriptor::{BlockDescriptor, FaceTextureDescriptor};

use super::{
    registries::{error::TextureNotFound, texture::TextureRegistry},
    texture::FaceTexture,
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
    pub default: BlockSubmodel,
}

impl BlockModel {
    pub fn from_descriptor(
        descriptor: &BlockDescriptor,
        registry: &TextureRegistry,
    ) -> Result<Self, SubmodelFromDescriptorError> {
        let default = BlockSubmodel::from_descriptor(&descriptor.default, registry)?;

        let mut directions = FaceMap::<BlockSubmodel>::new();
        for (face, desc) in descriptor.directions.iter() {
            if let Some(desc) = desc {
                let submodel = BlockSubmodel::from_descriptor(desc, registry)?;
                directions.set(face, submodel);
            }
        }

        Ok(Self {
            default,
            directions,
        })
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct BlockSubmodel([FaceTexture; 6]);

impl BlockSubmodel {
    pub fn from_face_map(map: FaceMap<FaceTexture>) -> Option<Self> {
        let mut arr: [Option<FaceTexture>; 6] = std::array::from_fn(|_i| None);

        for (face, tex) in map.iter() {
            arr[face.as_usize()] = Some(tex.copied()?);
        }

        Some(Self(arr.map(Option::unwrap)))
    }

    pub fn from_descriptor(
        map: &FaceMap<FaceTextureDescriptor>,
        registry: &TextureRegistry,
    ) -> Result<Self, SubmodelFromDescriptorError> {
        let mut textures = FaceMap::<FaceTexture>::new();

        for face in Face::FACES {
            let desc = map
                .get(face)
                .ok_or(SubmodelFromDescriptorError::MissingFace(face))?;
            let texture = registry
                .get_id(&desc.label)
                .ok_or_else(|| TextureNotFound(desc.label.clone()))?;

            textures.set(face, FaceTexture::new_rotated(texture, desc.rotation));
        }

        Ok(Self::from_face_map(textures).unwrap())
    }

    pub fn get_texture(&self, face: Face) -> FaceTexture {
        self.0[face.as_usize()]
    }
}

impl BlockModel {
    pub fn submodel(&self, direction: Face) -> &BlockSubmodel {
        self.directions.get(direction).unwrap_or(&self.default)
    }

    pub fn default_submodel(&self) -> &BlockSubmodel {
        &self.default
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
