use std::str::FromStr;

use crate::{
    data::{
        error::{
            BlockModelDescriptorParseError, FaceTextureDescriptorParseError,
            FaceTextureRotationParseError, RotatedTextureDescriptorParseError,
        },
        registries::{error::TextureNotFound, texture::TextureRegistry, Registry},
        texture::{FaceTexture, FaceTextureRotation},
        tile::{Face, Transparency},
        voxel::rotations::BlockModelFace,
    },
    util::FaceMap,
};

use super::{rotations::BlockModelFaceMap, BlockModel, VoxelModel};

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
                let mut textures = BlockModelFaceMap::new();

                for face in BlockModelFace::FACES {
                    if let Some(tex_desc) = model.textures.get(face) {
                        let Some(texture) = texture_registry.get_by_label(&tex_desc.label) else {
                            return Err(TextureNotFound(tex_desc.label.clone()));
                        };

                        textures.set(
                            face,
                            FaceTexture::new_rotated(texture.texture_pos, tex_desc.rotation),
                        );
                    }
                }

                Ok(VoxelModel::Block(BlockModel { textures }))
            }

            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(try_from = "UnparsedBlockModelDescriptor")]
pub struct BlockModelDescriptor {
    pub textures: BlockModelFaceMap<FaceTextureDescriptor>,
}

impl TryFrom<UnparsedBlockModelDescriptor> for BlockModelDescriptor {
    type Error = BlockModelDescriptorParseError;

    fn try_from(value: UnparsedBlockModelDescriptor) -> Result<Self, Self::Error> {
        todo!()
    }
}

#[derive(serde::Deserialize)]
struct UnparsedBlockModelDescriptor {
    faces: BlockModelFaceMap<FaceTextureDescriptor>,
    rotation: FaceMap<FaceMap<RotatedTextureDescriptor>>,
}

#[derive(serde::Deserialize)]
#[serde(try_from = "UnparsedRotatedTextureDescriptor")]
enum RotatedTextureDescriptor {
    SelfFace {
        face: BlockModelFace,
        rotation: FaceTextureRotation,
    },
    OtherTexture {
        label: String,
        rotation: FaceTextureRotation,
    },
}

impl TryFrom<UnparsedRotatedTextureDescriptor> for RotatedTextureDescriptor {
    type Error = RotatedTextureDescriptorParseError;

    fn try_from(value: UnparsedRotatedTextureDescriptor) -> Result<Self, Self::Error> {
        let string = value.0;

        if let Some(tex) = string.strip_prefix("self:") {
            let (face, rotation) = tex
                .split_once(':')
                .map(|(f, r)| {
                    (
                        BlockModelFace::from_str(f),
                        FaceTextureRotation::from_str(r),
                    )
                })
                .unwrap_or_else(|| {
                    (
                        BlockModelFace::from_str(tex),
                        Ok(FaceTextureRotation::default()),
                    )
                });

            let face = face?;
            let rotation = rotation?;

            Ok(Self::SelfFace { face, rotation })
        } else {
            let (label, rotation) = string
                .split_once(':')
                .map(|(lbl, r)| (lbl.to_string(), FaceTextureRotation::from_str(r)))
                .unwrap_or_else(|| (string, Ok(FaceTextureRotation::default())));

            let rotation = rotation?;

            Ok(Self::OtherTexture { label, rotation })
        }
    }
}

#[derive(serde::Deserialize)]
struct UnparsedRotatedTextureDescriptor(String);

#[derive(serde::Deserialize, Debug, Clone, PartialEq, dm::Constructor)]
#[serde(try_from = "UnparsedRotatedTextureDescriptor")]
pub struct FaceTextureDescriptor {
    label: String,
    rotation: FaceTextureRotation,
}

impl TryFrom<UnparsedRotatedTextureDescriptor> for FaceTextureDescriptor {
    type Error = FaceTextureDescriptorParseError;

    fn try_from(value: UnparsedRotatedTextureDescriptor) -> Result<Self, Self::Error> {
        let string = value.0;

        match string.split_once(':') {
            Some((texture, rotation)) => {
                let rotation = FaceTextureRotation::from_str(rotation)?;
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

impl BlockModelDescriptor {
    pub fn filled(label: String) -> Self {
        Self {
            textures: BlockModelFaceMap::from_fn(|_| {
                Some(FaceTextureDescriptor {
                    label: label.clone(),
                    rotation: Default::default(),
                })
            }),
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {

    use crate::{
        data::{
            texture::FaceTextureRotation,
            tile::{Face, Transparency},
            voxel::descriptor::{
                BlockModelDescriptor, FaceTextureDescriptor, VariantDescriptor,
                VoxelModelDescriptor,
            },
        },
        util::FaceMap,
    };

    #[test]
    #[ignore]
    fn deserialize_variant_descriptor() {
        let raw = br#"
        {
            trans: opaque,
            model: {
                type: "block",
                faces: {
                    up: "tex1:0",
                    down: "tex1:0",
                    left: "tex1:0",
                    right: "tex1:0",
                    front: "tex1:0",
                    back: "tex1:0",
                },
            }
        }
        "#;

        let raw = br#"
        {
            trans: opaque,
            model: {
                type: "block",
                faces: {
                    up: "tex1:0",
                    down: "tex1:0",
                    left: "tex1:0",
                    right: "tex1:0",
                    front: "tex1:0",
                    back: "tex1:0",
                },
                rotation: {
                    north: {
                        north: "self:up",
                        south: "self:down",
                        east: "self:right",
                        west: "self:left",
                        top: "tex1:-1",
                        bottom: "tex2",
                    }
                }
            }
        }
        "#;

        todo!();

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
