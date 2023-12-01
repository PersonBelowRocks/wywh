use std::borrow::Cow;

use crate::{
    data::{
        error::RotatedTextureDescriptorParseError,
        registries::{error::TextureNotFound, texture::TextureRegistry, Registry},
        texture::{FaceTexture, FaceTextureRotation},
        tile::{Face, Transparency},
    },
    util::FaceMap,
};

use super::{BlockModel, VoxelModel};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VariantDescriptor {
    pub model: Option<VoxelModelDescriptor>,
    #[serde(alias = "trans")]
    pub transparency: Transparency,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[non_exhaustive]
pub enum VoxelModelDescriptor {
    Block(BlockModelDescriptor),
}

impl VoxelModelDescriptor {
    pub fn create_voxel_model(
        &self,
        texture_registry: &TextureRegistry,
    ) -> Result<VoxelModel, TextureNotFound> {
        match self {
            VoxelModelDescriptor::Block(model) => {
                let mut textures = FaceMap::new();

                for face in Face::FACES {
                    if let Some(tex_desc) = model.textures.get(face) {
                        let Some(texture) = texture_registry.get_by_label(&tex_desc.label) else {
                            return Err(TextureNotFound(tex_desc.label.clone()));
                        };

                        textures.set(
                            face,
                            FaceTexture::new_rotated(texture.texture_pos, tex_desc.rotation),
                        )
                    }
                }

                Ok(VoxelModel::Block(BlockModel { textures }))
            }

            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct BlockModelDescriptor {
    pub textures: FaceMap<RotatedTextureDescriptor>,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(try_from = "UnparsedRotatedTextureDescriptor")]
pub struct RotatedTextureDescriptor {
    label: String,
    rotation: FaceTextureRotation,
}

impl TryFrom<UnparsedRotatedTextureDescriptor> for RotatedTextureDescriptor {
    type Error = RotatedTextureDescriptorParseError;

    fn try_from(value: UnparsedRotatedTextureDescriptor) -> Result<Self, Self::Error> {
        type E = RotatedTextureDescriptorParseError;

        let string = value.0;

        match string.split_once(':') {
            Some((texture, rotation)) => {
                let rotation =
                    FaceTextureRotation::from_str(rotation).ok_or(E::new(string.clone()))?;
                Ok(Self {
                    label: texture.to_string(),
                    rotation,
                })
            }
            None => {
                return Ok(Self {
                    label: string,
                    rotation: Default::default(),
                })
            }
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
struct UnparsedRotatedTextureDescriptor(String);

impl BlockModelDescriptor {
    pub fn filled(label: String) -> Self {
        let mut textures = FaceMap::new();
        for face in Face::FACES {
            textures.set(
                face,
                RotatedTextureDescriptor {
                    label: label.clone(),
                    rotation: Default::default(),
                },
            );
        }

        Self { textures }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn deserialize_variant_descriptor() {
        let raw = r#"
        {
            trans: opaque,
            
        }
        "#;

        todo!()
    }
}
