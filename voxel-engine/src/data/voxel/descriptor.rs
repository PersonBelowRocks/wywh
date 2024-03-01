use std::str::FromStr;

use crate::{
    data::{
        error::{FaceTextureDescParseError, SubmodelFaceTextureDescParseError},
        resourcepath::ResourcePath,
        texture::FaceTextureRotation,
        tile::Transparency,
    },
    util::FaceMap,
};

use super::rotations::{BlockModelFace, BlockModelFaceMap};

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockVariantDescriptor {
    pub options: BlockOptions,
    #[serde(flatten)]
    pub model: BlockModelDescriptor,
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
    use crate::data::resourcepath::rpath;

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
}
