use crate::{
    data::{
        registries::{error::TextureNotFound, texture::TextureRegistry, Registry},
        texture::{FaceTexture, FaceTextureRotation},
        tile::{Face, Transparency},
    },
    util::FaceMap,
};

use super::{BlockModel, VoxelModel};

pub struct VariantDescriptor<'lbl, 'mdl> {
    pub label: &'lbl str,
    pub model: Option<VoxelModelDescriptor<'mdl>>,
    pub transparency: Transparency,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum VoxelModelDescriptor<'a> {
    Block(BlockModelDescriptor<'a>),
}

impl<'a> VoxelModelDescriptor<'a> {
    pub fn create_voxel_model(
        &self,
        texture_registry: &TextureRegistry,
    ) -> Result<VoxelModel, TextureNotFound> {
        match self {
            VoxelModelDescriptor::Block(model) => {
                let mut textures = FaceMap::new();

                for face in Face::FACES {
                    if let Some(&label) = model.textures.get(face) {
                        let rotation = *model.rotations.get(face).unwrap();
                        let texture = texture_registry
                            .get_by_label(label)
                            .ok_or(TextureNotFound(label.into()))?;

                        textures.set(
                            face,
                            FaceTexture::new_rotated(texture.texture_pos, rotation),
                        )
                    }
                }

                Ok(VoxelModel::Block(BlockModel { textures }))
            }

            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockModelDescriptor<'a> {
    pub textures: FaceMap<&'a str>,
    pub rotations: FaceMap<FaceTextureRotation>,
}

impl<'a> BlockModelDescriptor<'a> {
    pub fn filled(label: &'a str) -> Self {
        Self {
            textures: FaceMap::filled(label),
            rotations: FaceMap::filled(FaceTextureRotation::default()),
        }
    }
}
