use crate::{
    data::{
        error::{FaceTextureParseError, SubmodelFaceTextureParseError},
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
    Model {
        tex: BlockModelFace,
        rotation: FaceTextureRotation,
    },
    Unique(FaceTextureDescriptor),
}

impl TryFrom<&str> for SubmodelFaceTextureDescriptor {
    type Error = SubmodelFaceTextureParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, dm::Constructor)]
#[serde(try_from = "&str")]
pub struct FaceTextureDescriptor {
    pub label: ResourcePath,
    pub rotation: FaceTextureRotation,
}

impl TryFrom<&str> for FaceTextureDescriptor {
    type Error = FaceTextureParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
