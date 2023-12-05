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

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct VariantDescriptor {
    pub model: Option<VoxelModelDescriptor>,
    #[serde(alias = "trans")]
    pub transparency: Transparency,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockModelDescriptor {
    #[serde(flatten)]
    pub textures: FaceMap<RotatedTextureDescriptor>,
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, dm::Constructor)]
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

    use crate::{
        data::{
            texture::FaceTextureRotation,
            tile::{Face, Transparency},
            voxel::descriptor::{
                BlockModelDescriptor, RotatedTextureDescriptor, VariantDescriptor,
                VoxelModelDescriptor,
            },
        },
        util::FaceMap,
    };

    #[test]
    #[ignore]
    fn deserialize_variant_descriptor() {
        todo!();

        let raw = br#"
        {
            trans: opaque,
            model: {
                type: "block",
                t: "tex1:up",
                bottom: "tex1:up",
                east: "tex2:down",
                w: "tex3:left",
            }
        }
        "#;

        // let textures = {
        //     let mut map = FaceMap::<RotatedTextureDescriptor>::new();
        //     map.set(
        //         Face::Top,
        //         RotatedTextureDescriptor::new("tex1".into(), FaceTextureRotation::Up),
        //     );
        //     map.set(
        //         Face::Bottom,
        //         RotatedTextureDescriptor::new("tex1".into(), FaceTextureRotation::Up),
        //     );
        //     map.set(
        //         Face::East,
        //         RotatedTextureDescriptor::new("tex2".into(), FaceTextureRotation::Down),
        //     );
        //     map.set(
        //         Face::West,
        //         RotatedTextureDescriptor::new("tex3".into(), FaceTextureRotation::Left),
        //     );

        //     map
        // };

        // let descriptor = VariantDescriptor {
        //     transparency: Transparency::Opaque,
        //     model: Some(VoxelModelDescriptor::Block(BlockModelDescriptor {
        //         textures,
        //     })),
        // };

        // let parsed_descriptor = deser_hjson::from_slice::<VariantDescriptor>(raw).unwrap();

        // assert_eq!(descriptor, parsed_descriptor);
    }
}
