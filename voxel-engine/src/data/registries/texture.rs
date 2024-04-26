use std::marker::PhantomData;

use bevy::{
    asset::{AssetId, Assets, Handle},
    ecs::system::Resource,
    render::texture::Image,
};
use indexmap::IndexMap;
use mip_texture_array::asset::MippedArrayTexture;
use mip_texture_array::MipArrayTextureBuilder;

use crate::data::{resourcepath::ResourcePath, texture::GpuFaceTexture};

#[cfg(test)]
use crate::data::resourcepath::rpath;

use super::{error::TextureRegistryError, Registry};

pub const TEXTURE_DIMENSIONS: u32 = 16;

pub type TexId = AssetId<Image>;

pub struct TextureRegistryLoader {
    textures: indexmap::IndexMap<ResourcePath, TexIdBundle, ahash::RandomState>,
}

#[derive(Clone)]
pub(crate) struct TexIdBundle {
    pub color: TexId,
    pub normal: Option<TexId>,
}

impl TextureRegistryLoader {
    pub fn new() -> Self {
        Self {
            textures: indexmap::IndexMap::with_hasher(ahash::RandomState::new()),
        }
    }

    pub fn register(&mut self, label: ResourcePath, texture: TexId, normal: Option<TexId>) {
        self.textures.insert(
            label.into(),
            TexIdBundle {
                color: texture,
                normal,
            },
        );
    }

    pub fn build_registry(
        self,
        textures: &Assets<Image>,
        array_textures: &mut Assets<MippedArrayTexture>,
    ) -> Result<TextureRegistry, TextureRegistryError> {
        // we map the asset id to the array texture index
        let mut color_id_to_idx = hb::HashMap::<AssetId<Image>, u32>::new();

        let color_arr_tex = {
            let mut builder = MipArrayTextureBuilder::new(TEXTURE_DIMENSIONS, true);
            builder.set_label(Some("color_array_texture"));

            for id in self.textures.values().cloned() {
                let id = id.color;

                let texarr_idx = builder.add_image(id, textures)? as u32;
                color_id_to_idx.insert(id, texarr_idx);
            }

            builder.finish(textures, array_textures)?
        };

        let mut normal_id_to_idx = hb::HashMap::<AssetId<Image>, u32>::new();

        let normal_arr_tex = {
            let mut builder = MipArrayTextureBuilder::new(TEXTURE_DIMENSIONS, false);
            builder.set_label(Some("normal_array_texture"));

            for id in self.textures.values().cloned() {
                let Some(id) = id.normal else {
                    continue;
                };

                let texarr_idx = builder.add_image(id, textures)? as u32;
                normal_id_to_idx.insert(id, texarr_idx);
            }

            builder.finish(textures, array_textures)?
        };

        let registry_map = {
            let mut map =
                IndexMap::<ResourcePath, AtlasIdxBundle, ahash::RandomState>::with_capacity_and_hasher(
                    self.textures.len(),
                    ahash::RandomState::new(),
                );

            map.reserve(self.textures.len());
            for (label, ids) in self.textures.into_iter() {
                let indices = AtlasIdxBundle {
                    color: *color_id_to_idx.get(&ids.color).unwrap(),
                    normal: ids.normal.map(|id| *normal_id_to_idx.get(&id).unwrap()),
                };

                map.insert(label, indices);
            }

            map
        };

        Ok(TextureRegistry {
            map: registry_map,
            color_atlas: color_arr_tex,
            normal_atlas: normal_arr_tex,
        })
    }
}

#[derive(Clone, Resource)]
pub struct TexregFaces(pub Vec<GpuFaceTexture>);

pub struct TextureRegistry {
    map: IndexMap<ResourcePath, AtlasIdxBundle, ahash::RandomState>,

    color_atlas: Handle<MippedArrayTexture>,
    normal_atlas: Handle<MippedArrayTexture>,
}

pub(crate) struct AtlasIdxBundle {
    pub color: u32,
    pub normal: Option<u32>,
}

#[cfg(test)]
impl TextureRegistry {
    pub const RPATH_TEX1: &'static str = "tex1";
    pub const TEX1: TextureId = TextureId::new(0);
    pub const RPATH_TEX2: &'static str = "tex2";
    pub const TEX2: TextureId = TextureId::new(1);
    pub const RPATH_TEX3: &'static str = "tex3";
    pub const TEX3: TextureId = TextureId::new(2);

    pub fn new_mock() -> Self {
        let mut map = IndexMap::with_hasher(ahash::RandomState::default());

        map.insert(
            rpath(Self::RPATH_TEX1),
            AtlasIdxBundle {
                color: 0,
                normal: None,
            },
        );

        map.insert(
            rpath(Self::RPATH_TEX2),
            AtlasIdxBundle {
                color: 1,
                normal: Some(0),
            },
        );

        map.insert(
            rpath(Self::RPATH_TEX3),
            AtlasIdxBundle {
                color: 2,
                normal: Some(1),
            },
        );

        Self {
            map,
            color_atlas: Handle::default(),
            normal_atlas: Handle::default(),
        }
    }
}

impl TextureRegistry {
    pub fn color_texture(&self) -> &Handle<MippedArrayTexture> {
        &self.color_atlas
    }

    pub fn normal_texture(&self) -> &Handle<MippedArrayTexture> {
        &self.normal_atlas
    }

    pub fn texture_scale(&self) -> f32 {
        // TODO: this should be configurable without recompiling so we can support textures of different resolutions
        16.0
    }

    pub fn face_texture_buffer(&self) -> Vec<GpuFaceTexture> {
        self.map
            .values()
            .map(|indices| {
                GpuFaceTexture::new(indices.color as u32, indices.normal.map(|v| v as u32))
            })
            .collect::<Vec<_>>()
    }
}

#[derive(Copy, Clone, Debug, dm::Constructor)]
pub struct TextureRegistryEntry<'a> {
    pub texture_idx: u32,
    pub normal_idx: Option<u32>,

    // Placeholder in case we wanna store some other funny stuff in here
    _data: PhantomData<&'a ()>,
}

impl<'a> TextureRegistryEntry<'a> {
    pub fn gpu_representation(&self) -> GpuFaceTexture {
        GpuFaceTexture::new(self.texture_idx, self.normal_idx)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, dm::Display)]
#[display(fmt="[texture:{:08}]", self.0)]
pub struct TextureId(u32);

impl TextureId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }

    #[cfg(test)]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl Registry for TextureRegistry {
    type Item<'a> = TextureRegistryEntry<'a>;
    type Id = TextureId;

    fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>> {
        Some(self.get_by_id(self.get_id(label)?))
    }

    fn get_by_id(&self, id: Self::Id) -> Self::Item<'_> {
        let map_idx = id.index();
        let indices = self.map.get_index(map_idx).unwrap().1;

        TextureRegistryEntry {
            texture_idx: indices.color as u32,
            normal_idx: indices.normal.map(|v| v as u32),
            _data: PhantomData,
        }
    }

    fn get_id(&self, label: &ResourcePath) -> Option<Self::Id> {
        self.map
            .get_index_of(label)
            .map(|idx| TextureId(idx as u32))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore]
    fn texture_registry_basics() {
        todo!()
    }
}
