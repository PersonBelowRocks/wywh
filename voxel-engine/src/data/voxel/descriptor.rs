use crate::{
    data::{
        registries::{texture::TextureRegistry, Registry, RegistryId},
        texture::{FaceTexture, FaceTextureRotation},
        tile::Transparency,
    },
    util::FaceMap,
};

use super::{BlockModel, VoxelModel};

pub struct VariantDescriptor<'lbl> {
    pub label: &'lbl str,
    pub model: Option<VoxelModelDescriptor>,
    pub transparency: Transparency,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum VoxelModelDescriptor {
    Block(BlockModelDescriptor),
}

impl VoxelModelDescriptor {
    pub fn create_voxel_model(&self, texture_registry: &TextureRegistry) -> VoxelModel {
        match self {
            VoxelModelDescriptor::Block(model) => {
                let textures = model.textures.map(|face, &id| {
                    let rotation = *model.rotations.get(face).unwrap();
                    let texture = texture_registry.get_by_id(id);

                    FaceTexture::new_rotated(texture.texture_pos, rotation)
                });

                VoxelModel::Block(BlockModel { textures })
            }

            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockModelDescriptor {
    pub textures: FaceMap<RegistryId<TextureRegistry>>,
    pub rotations: FaceMap<FaceTextureRotation>,
}
