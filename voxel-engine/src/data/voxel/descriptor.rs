use crate::{
    data::{
        error::VoxelModelCreationError, registries::texture::TextureRegistry,
        resourcepath::ResourcePath, texture::FaceTextureRotation, tile::Transparency,
    },
    util::FaceMap,
};

use super::{
    serialization::{UnparsedBlockModelDescriptor, UnparsedRotatedTextureDescriptor},
    BlockModel, VoxelModel,
};

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
    Block(BlockDescriptor),
}

impl VoxelModelDescriptor {
    pub fn create_voxel_model(
        &self,
        registry: &TextureRegistry,
    ) -> Result<VoxelModel, VoxelModelCreationError> {
        match self {
            VoxelModelDescriptor::Block(descriptor) => {
                Ok(BlockModel::from_descriptor(descriptor, registry).map(VoxelModel::Block)?)
            }

            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(try_from = "UnparsedBlockModelDescriptor")]
pub struct BlockDescriptor {
    pub directions: FaceMap<FaceMap<FaceTextureDescriptor>>,
    pub default: FaceMap<FaceTextureDescriptor>,
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, dm::Constructor)]
#[serde(try_from = "UnparsedRotatedTextureDescriptor")]
pub struct FaceTextureDescriptor {
    pub label: ResourcePath,
    pub rotation: FaceTextureRotation,
}

impl BlockDescriptor {
    pub fn filled(label: ResourcePath) -> Self {
        Self {
            directions: FaceMap::new(),
            default: FaceMap::from_fn(|_| {
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
            resourcepath::{rpath, ResourcePath},
            texture::FaceTextureRotation,
            tile::{Face, Transparency},
            voxel::descriptor::{
                BlockDescriptor, FaceTextureDescriptor, VariantDescriptor, VoxelModelDescriptor,
            },
        },
        util::FaceMap,
    };

    #[test]
    fn deserialize_variant_descriptor() {
        let raw = br#"
        {
            trans: opaque,
            model: {
                type: "block",
                faces: {
                    up: "tex1:1",
                    down: "stupid_textures.tex1:1",
                    left: "stupid_textures.tex2:-1",
                    right: "tex2:0",
                    front: "tex2:2",
                    back: "tex1:0",
                },
                rotation: {
                    north: {
                        north: "self:up",
                        south: "self:down",
                        east: "self:right",
                        west: "self:left",
                        top: "tex4:-1",
                        bottom: "tex3",
                    }
                }
            }
        }
        "#;

        let variant = deser_hjson::from_slice::<VariantDescriptor>(raw).unwrap();

        assert_eq!(Transparency::Opaque, variant.transparency);
        let Some(VoxelModelDescriptor::Block(model)) = variant.model else {
            panic!("didn't match block model")
        };

        let direction = model.directions.get(Face::North).unwrap();

        assert_eq!(
            FaceTextureRotation::new(1),
            direction.get(Face::North).unwrap().rotation
        );
        assert_eq!(rpath("tex1"), direction.get(Face::North).unwrap().label);

        assert_eq!(
            FaceTextureRotation::new(1),
            direction.get(Face::South).unwrap().rotation
        );
        assert_eq!(
            rpath("stupid_textures.tex1"),
            direction.get(Face::South).unwrap().label
        );

        assert_eq!(
            FaceTextureRotation::new(0),
            direction.get(Face::East).unwrap().rotation
        );
        assert_eq!(rpath("tex2"), direction.get(Face::East).unwrap().label);

        assert_eq!(
            FaceTextureRotation::new(-1),
            direction.get(Face::West).unwrap().rotation
        );
        assert_eq!(
            rpath("stupid_textures.tex2"),
            direction.get(Face::West).unwrap().label
        );

        assert_eq!(
            FaceTextureRotation::new(-1),
            direction.get(Face::Top).unwrap().rotation
        );
        assert_eq!(rpath("tex4"), direction.get(Face::Top).unwrap().label);

        assert_eq!(
            FaceTextureRotation::default(),
            direction.get(Face::Bottom).unwrap().rotation
        );
        assert_eq!(rpath("tex3"), direction.get(Face::Bottom).unwrap().label);

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
