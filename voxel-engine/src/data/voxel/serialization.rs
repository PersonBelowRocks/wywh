use std::{marker::PhantomData, str::FromStr};

use crate::{
    data::{
        error::{
            BlockModelDescriptorParseError, FaceTextureDescriptorParseError,
            RotatedTextureDescriptorParseError,
        },
        resourcepath::{rpath, ResourcePath},
        texture::FaceTextureRotation,
        tile::Face,
    },
    util::FaceMap,
};

use super::{
    descriptor::{BlockDescriptor, FaceTextureDescriptor},
    rotations::{BlockModelFace, BlockModelFaceMap, BlockModelRotation},
};

impl<T: serde::Serialize> serde::Serialize for BlockModelFaceMap<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut sermap = serializer.serialize_map(Some(self.len()))?;

        self.map(|face, value| sermap.serialize_entry(&face, value));

        sermap.end()
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for BlockModelFaceMap<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(BlockModelFaceMapVisitor(PhantomData))
    }
}

struct BlockModelFaceMapVisitor<T>(PhantomData<T>);

impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for BlockModelFaceMapVisitor<T> {
    type Value = BlockModelFaceMap<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map with keyed with the faces of a cube")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut out = BlockModelFaceMap::<T>::new();

        while let Some((face, value)) = map.next_entry::<BlockModelFace, T>()? {
            out.set(face, value);
        }

        Ok(out)
    }
}

impl TryFrom<UnparsedBlockModelDescriptor> for BlockDescriptor {
    type Error = BlockModelDescriptorParseError;

    fn try_from(unparsed: UnparsedBlockModelDescriptor) -> Result<Self, Self::Error> {
        let default_direction = BlockModelRotation::new(Face::North, Face::Top).unwrap();

        let mut out = BlockDescriptor {
            transparency: todo!(),
            directions: FaceMap::new(),
            default: FaceMap::new(),
        };

        for face in BlockModelFace::FACES {
            let cardinal = default_direction.get_cardinal_face(face);

            // TODO: this might be fallible? check if it is and fix it accordingly!
            out.default
                .set(cardinal, unparsed.faces.get(face).unwrap().clone());
        }

        for pair in unparsed.rotations.iter() {
            let (direction, Some(model)) = pair else {
                continue;
            };

            let mut direction_layout = FaceMap::new();

            for (face, tex) in model.iter() {
                let tex = tex.ok_or_else(|| {
                    BlockModelDescriptorParseError::MissingFaceInDirection { direction, face }
                })?;

                match tex {
                    RotatedTextureDescriptor::SelfFace {
                        block_model_face,
                        rotation,
                    } => {
                        let mut tex = unparsed
                            .faces
                            .get(*block_model_face)
                            .ok_or(BlockModelDescriptorParseError::MissingBlockModelFace(
                                *block_model_face,
                            ))?
                            .clone();

                        tex.rotation += *rotation;

                        direction_layout.set(face, tex);
                    }
                    RotatedTextureDescriptor::OtherTexture { label, rotation } => {
                        direction_layout.set(
                            face,
                            FaceTextureDescriptor {
                                label: label.clone(),
                                rotation: *rotation,
                            },
                        );
                    }
                }
            }

            out.directions.set(direction, direction_layout);
        }

        Ok(out)
    }
}

#[derive(serde::Deserialize)]
pub(super) struct UnparsedBlockModelDescriptor {
    faces: BlockModelFaceMap<FaceTextureDescriptor>,
    #[serde(alias = "rotation")]
    #[serde(default)]
    rotations: FaceMap<FaceMap<RotatedTextureDescriptor>>,
}

#[derive(serde::Deserialize)]
#[serde(try_from = "UnparsedRotatedTextureDescriptor")]
enum RotatedTextureDescriptor {
    SelfFace {
        block_model_face: BlockModelFace,
        rotation: FaceTextureRotation,
    },
    OtherTexture {
        label: ResourcePath,
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

            Ok(Self::SelfFace {
                block_model_face: face,
                rotation,
            })
        } else {
            let (label, rotation) = string
                .split_once(':')
                .map(|(lbl, r)| (lbl.to_string(), FaceTextureRotation::from_str(r)))
                .unwrap_or_else(|| (string, Ok(FaceTextureRotation::default())));

            let rotation = rotation?;

            Ok(Self::OtherTexture {
                label: rpath(label.as_str()),
                rotation,
            })
        }
    }
}

#[derive(serde::Deserialize)]
pub(super) struct UnparsedRotatedTextureDescriptor(String);

impl TryFrom<UnparsedRotatedTextureDescriptor> for FaceTextureDescriptor {
    type Error = FaceTextureDescriptorParseError;

    fn try_from(value: UnparsedRotatedTextureDescriptor) -> Result<Self, Self::Error> {
        let string = value.0;

        match string.split_once(':') {
            Some((texture, rotation)) => {
                let rotation = FaceTextureRotation::from_str(rotation)?;
                Ok(Self {
                    label: rpath(texture),
                    rotation,
                })
            }
            None => {
                return Ok(Self {
                    label: rpath(string.as_str()),
                    rotation: Default::default(),
                })
            }
        }
    }
}
