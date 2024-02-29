use std::str::FromStr;

use itertools::Itertools;
use nom::{
    bytes::complete::tag,
    character::complete::{alphanumeric1, anychar},
    multi::separated_list1,
};

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
    #[serde(alias = "trans")]
    pub transparency: Transparency,
    #[serde(default)]
    pub subdividable: bool,
    #[serde(flatten)]
    pub model: BlockModelDescriptor,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockModelDescriptor {
    pub model: BlockModelFaceMap<FaceTextureDescriptor>,
    pub directions: FaceMap<SubmodelDescriptor>,
}

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct SubmodelDescriptor(FaceMap<SubmodelFaceTextureDescriptor>);

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(try_from = "&str")]
pub enum SubmodelFaceTextureDescriptor {
    SelfFace {
        face: BlockModelFace,
        rotation: FaceTextureRotation,
    },
    Unique(FaceTextureDescriptor),
}

impl TryFrom<&str> for SubmodelFaceTextureDescriptor {
    type Error = SubmodelFaceTextureDescParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut parts = value.split(':');

        let self_prefix_or_rpath = parts.next().ok_or_else(|| Self::Error::new(value))?;

        if self_prefix_or_rpath == "self" {
            let model_face = parts
                .next()
                .ok_or_else(|| Self::Error::new(value))
                .and_then(|s| BlockModelFace::from_str(s).map_err(|_| Self::Error::new(value)))?;

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
                FaceTextureDescriptor::try_from(value).map_err(|_| Self::Error::new(value))?,
            ))
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, dm::Constructor)]
#[serde(try_from = "&str")]
pub struct FaceTextureDescriptor {
    pub rpath: ResourcePath,
    pub rotation: FaceTextureRotation,
}

impl TryFrom<&str> for FaceTextureDescriptor {
    type Error = FaceTextureDescParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut parts = value.split(':');

        let rpath = parts.next().ok_or_else(|| Self::Error::new(value))?;
        let rotation: FaceTextureRotation = parts
            .next()
            .map(FaceTextureRotation::from_str)
            .unwrap_or(Ok(FaceTextureRotation::default()))
            .map_err(|_| Self::Error::new(value))?;

        Ok(Self {
            rpath: ResourcePath::parse(rpath).map_err(|_| Self::Error::new(value))?,
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
        T: std::fmt::Debug + PartialEq + TryFrom<&'a str>,
    {
        assert_eq!(Ok(desc), T::try_from(s))
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
}
