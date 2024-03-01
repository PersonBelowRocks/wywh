use std::str::FromStr;

use crate::{
    data::{
        error::{
            BlockModelCreationError, FaceTextureDescParseError, SubmodelFaceTextureDescParseError,
        },
        registries::{error::TextureNotFound, texture::TextureRegistry, Registry},
        resourcepath::ResourcePath,
        texture::{FaceTexture, FaceTextureRotation},
        tile::{Face, Transparency},
        voxel::SubmodelFaceTexture,
    },
    util::FaceMap,
};

use super::{
    rotations::{BlockModelFace, BlockModelFaceMap},
    BlockModel, BlockSubmodel,
};

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockVariantDescriptor {
    pub options: BlockOptions,
    #[serde(flatten)]
    pub model: Option<BlockModelDescriptor>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockOptions {
    pub transparency: Transparency,
    #[serde(default)]
    pub subdividable: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockModelDescriptor {
    pub model: BlockModelFaceMap<FaceTextureDescriptor>,
    pub directions: FaceMap<SubmodelDescriptor>,
}

impl BlockModelDescriptor {
    pub fn create_block_model<R>(&self, registry: &R) -> Result<BlockModel, BlockModelCreationError>
    where
        R: Registry<Id = <TextureRegistry as Registry>::Id>,
    {
        let model_face_map = {
            let mut map = BlockModelFaceMap::<FaceTexture>::new();
            for face in BlockModelFace::FACES {
                let tex_desc = self
                    .model
                    .get(face)
                    .ok_or(BlockModelCreationError::MissingModelFace(face))?;

                let tex_id = registry.get_id(&tex_desc.rpath).ok_or_else(|| {
                    BlockModelCreationError::TextureNotFound(tex_desc.rpath.clone())
                })?;

                map.set(face, FaceTexture::new_rotated(tex_id, tex_desc.rotation));
            }

            map
        };

        let directions = {
            let mut map = FaceMap::<BlockSubmodel>::new();

            for direction in Face::FACES {
                let Some(submodel_desc) = self.directions.get(direction) else {
                    continue;
                };

                let mut submodel_arr = [None::<SubmodelFaceTexture>; 6];

                for submodel_face in Face::FACES {
                    let submodel_face_tex_desc = submodel_desc.0.get(submodel_face).ok_or(
                        BlockModelCreationError::MissingDirectionFace(direction, submodel_face),
                    )?;

                    submodel_arr[submodel_face as usize] = Some(match submodel_face_tex_desc {
                        SubmodelFaceTextureDescriptor::SelfFace { face, rotation } => {
                            SubmodelFaceTexture::SelfFace {
                                face: *face,
                                rotation: *rotation,
                            }
                        }
                        SubmodelFaceTextureDescriptor::Unique(desc) => {
                            SubmodelFaceTexture::Unique(FaceTexture::new_rotated(
                                registry.get_id(&desc.rpath).ok_or(
                                    BlockModelCreationError::TextureNotFound(desc.rpath.clone()),
                                )?,
                                desc.rotation,
                            ))
                        }
                    });
                }

                map.set(
                    direction,
                    BlockSubmodel::from_arr(submodel_arr.map(Option::unwrap)),
                );
            }

            map
        };

        Ok(BlockModel {
            model: model_face_map,
            directions,
        })
    }
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct SubmodelDescriptor(FaceMap<SubmodelFaceTextureDescriptor>);

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "String")]
pub enum SubmodelFaceTextureDescriptor {
    SelfFace {
        face: BlockModelFace,
        rotation: FaceTextureRotation,
    },
    Unique(FaceTextureDescriptor),
}

impl TryFrom<String> for SubmodelFaceTextureDescriptor {
    type Error = SubmodelFaceTextureDescParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut parts = value.split(':');

        let self_prefix_or_rpath = parts
            .next()
            .ok_or_else(|| Self::Error::new(value.clone()))?;

        if self_prefix_or_rpath == "self" {
            let model_face = parts
                .next()
                .ok_or_else(|| Self::Error::new(value.clone()))
                .and_then(|s| {
                    BlockModelFace::from_str(s).map_err(|_| Self::Error::new(value.clone()))
                })?;

            let rotation: FaceTextureRotation = parts
                .next()
                .map(FaceTextureRotation::from_str)
                .unwrap_or(Ok(FaceTextureRotation::default()))
                .map_err(|_| Self::Error::new(value))?;

            Ok(Self::SelfFace {
                face: model_face,
                rotation,
            })
        } else {
            Ok(Self::Unique(
                FaceTextureDescriptor::try_from(value.clone())
                    .map_err(|_| Self::Error::new(value.clone()))?,
            ))
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, dm::Constructor)]
#[serde(try_from = "String")]
pub struct FaceTextureDescriptor {
    pub rpath: ResourcePath,
    pub rotation: FaceTextureRotation,
}

impl TryFrom<String> for FaceTextureDescriptor {
    type Error = FaceTextureDescParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut parts = value.split(':');

        let rpath = parts
            .next()
            .ok_or_else(|| Self::Error::new(value.clone()))?;

        let rotation: FaceTextureRotation = parts
            .next()
            .map(FaceTextureRotation::from_str)
            .unwrap_or(Ok(FaceTextureRotation::default()))
            .map_err(|_| Self::Error::new(value.clone()))?;

        Ok(Self {
            rpath: ResourcePath::parse(rpath).map_err(|_| Self::Error::new(value.clone()))?,
            rotation,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::data::{registries::texture::TextureId, resourcepath::rpath};

    use super::*;

    fn test<'a, T>(desc: T, s: &'a str)
    where
        T::Error: PartialEq + std::fmt::Debug,
        T: std::fmt::Debug + PartialEq + TryFrom<String>,
    {
        assert_eq!(Ok(desc), T::try_from(s.to_string()))
    }

    #[test]
    fn parse_face_texture() {
        test(
            FaceTextureDescriptor {
                rpath: rpath("test.example.rpath"),
                rotation: FaceTextureRotation::new(2),
            },
            "test.example.rpath:2",
        );

        test(
            FaceTextureDescriptor {
                rpath: rpath("test.example.rpath"),
                rotation: FaceTextureRotation::new(-1),
            },
            "test.example.rpath:-1",
        );

        test(
            FaceTextureDescriptor {
                rpath: rpath("no_dots"),
                rotation: FaceTextureRotation::new(0),
            },
            "no_dots:0",
        );

        test(
            FaceTextureDescriptor {
                rpath: rpath("no_rotation"),
                rotation: FaceTextureRotation::default(),
            },
            "no_rotation",
        );
    }

    #[test]
    fn parse_submodel_face_texture_unique() {
        test(
            SubmodelFaceTextureDescriptor::Unique(FaceTextureDescriptor {
                rpath: rpath("example.rpath"),
                rotation: FaceTextureRotation::default(),
            }),
            "example.rpath",
        );

        test(
            SubmodelFaceTextureDescriptor::Unique(FaceTextureDescriptor {
                rpath: rpath("with_rotation"),
                rotation: FaceTextureRotation::new(-1),
            }),
            "with_rotation:-1",
        );
    }

    #[test]
    fn parse_submodel_face_texture_self_face() {
        test(
            SubmodelFaceTextureDescriptor::SelfFace {
                face: BlockModelFace::Up,
                rotation: FaceTextureRotation::new(2),
            },
            "self:up:2",
        );

        test(
            SubmodelFaceTextureDescriptor::SelfFace {
                face: BlockModelFace::Down,
                rotation: FaceTextureRotation::new(2),
            },
            "self:down:2",
        );

        test(
            SubmodelFaceTextureDescriptor::SelfFace {
                face: BlockModelFace::Left,
                rotation: FaceTextureRotation::new(-1),
            },
            "self:left:-1",
        );
    }

    #[test]
    fn parse_block_model_descriptor() {
        let s = r#"
            [model]
            up = "example.face.up"
            down = "example.face.down"
            left = "example.face.left:-1"
            right = "example.face.right:-1"
            front = "example.face.front"
            back = "example.face.back"

            [directions.east]
            top = "self:up"
            bottom = "self:down"
            north = "self:left:1"
            east = "self:front"
            south = "self:right:1"
            west = "facing.east.western.face"

            [directions.west]
            west = "self:front"
        "#;

        let de = toml::from_str::<BlockModelDescriptor>(s).unwrap();

        assert_eq!(
            Some(&FaceTextureDescriptor {
                rpath: rpath("example.face.front"),
                rotation: FaceTextureRotation::default()
            }),
            de.model.get(BlockModelFace::Front)
        );

        assert_eq!(
            Some(&FaceTextureDescriptor {
                rpath: rpath("example.face.left"),
                rotation: FaceTextureRotation::new(-1)
            }),
            de.model.get(BlockModelFace::Left)
        );
    }

    struct Reg;

    impl Registry for Reg {
        type Id = TextureId;
        type Item<'a> = ();

        fn get_by_id(&self, id: Self::Id) -> Self::Item<'_> {
            unreachable!()
        }

        fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>> {
            unreachable!()
        }

        fn get_id(&self, label: &ResourcePath) -> Option<Self::Id> {
            let s = label.string();

            match s.as_str() {
                "rpath.one" => Some(TextureId::new(1)),
                "rpath.two" => Some(TextureId::new(2)),
                "rpath.three" => Some(TextureId::new(3)),
                _ => None,
            }
        }
    }

    #[test]
    fn build_block_model() {
        let desc = BlockModelDescriptor {
            model: BlockModelFaceMap::from_fn(|_| {
                Some(FaceTextureDescriptor {
                    rpath: rpath("rpath.one"),
                    rotation: Default::default(),
                })
            }),
            directions: {
                let mut map = FaceMap::<SubmodelDescriptor>::new();

                map.set(
                    Face::East,
                    SubmodelDescriptor(FaceMap::from_fn(|_| {
                        Some(SubmodelFaceTextureDescriptor::SelfFace {
                            face: BlockModelFace::Back,
                            rotation: FaceTextureRotation::new(-1),
                        })
                    })),
                );

                map.set(
                    Face::West,
                    SubmodelDescriptor(FaceMap::from_fn(|face| {
                        Some(if face == Face::North {
                            SubmodelFaceTextureDescriptor::Unique(FaceTextureDescriptor::new(
                                rpath("rpath.two"),
                                FaceTextureRotation::new(2),
                            ))
                        } else {
                            SubmodelFaceTextureDescriptor::SelfFace {
                                face: BlockModelFace::Front,
                                rotation: FaceTextureRotation::default(),
                            }
                        })
                    })),
                );

                map
            },
        };

        let block_model = desc.create_block_model(&Reg).unwrap();

        for face in BlockModelFace::FACES {
            assert_eq!(TextureId::new(1), block_model.model.get(face).unwrap().id)
        }

        // TODO: more tests, test the northern face in the western direction
    }
}
