use std::str::FromStr;

use crate::{
    data::{
        error::{BlockModelCreationError, FaceTextureDescParseError},
        registries::{block::BlockOptions, texture::TextureRegistry, Registry},
        resourcepath::ResourcePath,
        texture::{FaceTexture, FaceTextureRotation},
        tile::Face,
    },
    util::FaceMap,
};

use super::BlockModel;

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockVariantDescriptor {
    pub options: BlockOptions,
    pub model: Option<BlockModelDescriptor>,
}

/// Describes a block model. This type does not point directly to the ID of textures in their registries
/// but rather stores the path that the textures are found at. Descriptors do not depend on registries,
/// and are the precursor to registry-dependant types like [`BlockModel`].
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct BlockModelDescriptor {
    pub faces: FaceMap<FaceTextureDescriptor>,
}

impl BlockModelDescriptor {
    /// Create a block model tied to the given registry based on this descriptor.
    #[inline]
    pub fn create_block_model<R>(&self, registry: &R) -> Result<BlockModel, BlockModelCreationError>
    where
        R: Registry<Id = <TextureRegistry as Registry>::Id>,
    {
        Ok(BlockModel {
            faces: {
                let mut map = FaceMap::<FaceTexture>::new();
                for face in Face::FACES {
                    let tex_desc = self
                        .faces
                        .get(face)
                        .ok_or(BlockModelCreationError::MissingFace(face))?;

                    let tex_id = registry.get_id(&tex_desc.rpath).ok_or_else(|| {
                        BlockModelCreationError::TextureNotFound(tex_desc.rpath.clone())
                    })?;

                    map.set(face, FaceTexture::new_rotated(tex_id, tex_desc.rotation));
                }

                map
            },
        })
    }
}

/// Describes the texture of a face in a block model.
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
    fn parse_block_model_descriptor() {
        let s = r#"
            [faces]
            top = "example.face.up"
            bottom = "example.face.down"
            west = "example.face.left:-1"
            east = "example.face.right:-1"
            north = "example.face.front"
            south = "example.face.back"
        "#;

        let de = toml::from_str::<BlockModelDescriptor>(s).unwrap();

        assert_eq!(
            rpath("example.face.up"),
            de.faces.get(Face::Top).unwrap().rpath
        );
        assert_eq!(
            rpath("example.face.left"),
            de.faces.get(Face::West).unwrap().rpath
        );
        assert_eq!(
            FaceTextureRotation::new(-1),
            de.faces.get(Face::West).unwrap().rotation
        );
    }

    struct Reg;

    impl Registry for Reg {
        type Id = TextureId;
        type Item<'a> = ();

        fn get_by_id(&self, _id: Self::Id) -> Self::Item<'_> {
            unreachable!()
        }

        fn get_by_label(&self, _label: &ResourcePath) -> Option<Self::Item<'_>> {
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
            faces: FaceMap::from_fn(|_| {
                Some(FaceTextureDescriptor::new(
                    rpath("rpath.one"),
                    FaceTextureRotation::new(0),
                ))
            }),
        };

        let block_model = desc.create_block_model(&Reg).unwrap();

        for face in Face::FACES {
            assert_eq!(TextureId::new(1), block_model.faces.get(face).unwrap().id)
        }

        // TODO: more tests, test the northern face in the western direction
    }
}
