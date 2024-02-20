use std::marker::PhantomData;

use bevy::{
    asset::{AssetId, Assets, Handle},
    ecs::system::Resource,
    log::info,
    math::Vec2,
    render::texture::Image,
    sprite::{TextureAtlas, TextureAtlasBuilder},
};
use indexmap::IndexMap;
use mip_texture_array::asset::MippedArrayTexture;
use mip_texture_array::MipArrayTextureBuilder;

use crate::data::{resourcepath::ResourcePath, texture::GpuFaceTexture};

use super::{error::TextureRegistryError, Registry, RegistryId};

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
            let mut builder = MipArrayTextureBuilder::new(TEXTURE_DIMENSIONS);

            for id in self.textures.values().cloned() {
                let id = id.color;

                let tex = textures
                    .get(id)
                    .ok_or(TextureRegistryError::TextureNotLoaded(id))?;

                let texarr_idx = builder.add_image(id, textures)? as u32;
                color_id_to_idx.insert(id, texarr_idx);
            }

            builder.finish(textures, array_textures)?
        };

        let mut normal_id_to_idx = hb::HashMap::<AssetId<Image>, u32>::new();

        let normal_arr_tex = {
            let mut builder = MipArrayTextureBuilder::new(TEXTURE_DIMENSIONS);
            for id in self.textures.values().cloned() {
                let Some(id) = id.normal else {
                    continue;
                };

                let tex = textures
                    .get(id)
                    .ok_or(TextureRegistryError::TextureNotLoaded(id))?;

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

            #[cfg(test)]
            colors: Vec::new(),
            #[cfg(test)]
            normals: Vec::new(),
        })
    }
}

#[cfg(test)]
pub struct TestTextureRegistryLoader {
    map: IndexMap<ResourcePath, AtlasIdxBundle, ahash::RandomState>,

    colors: Vec<Vec2>,
    normals: Vec<Vec2>,
}

#[cfg(test)]
impl TestTextureRegistryLoader {
    pub fn new() -> Self {
        Self {
            map: Default::default(),
            colors: Vec::new(),
            normals: Vec::new(),
        }
    }

    pub fn add(&mut self, rpath: ResourcePath, color: Vec2, normal: Option<Vec2>) {
        let bundle = AtlasIdxBundle {
            color: todo!(),
            normal: todo!(),
        };

        self.colors.push(color);
        if let Some(normal) = normal {
            self.normals.push(normal);
        }

        self.map.insert(rpath, bundle);
    }

    pub fn build(self) -> TextureRegistry {
        TextureRegistry {
            map: self.map,

            color_atlas: todo!(),
            normal_atlas: todo!(),

            colors: self.colors,
            normals: self.normals,
        }
    }
}

#[derive(Clone, Resource)]
pub struct TexregFaces(pub Vec<GpuFaceTexture>);

pub struct TextureRegistry {
    map: IndexMap<ResourcePath, AtlasIdxBundle, ahash::RandomState>,

    color_atlas: Handle<MippedArrayTexture>,
    normal_atlas: Handle<MippedArrayTexture>,

    #[cfg(test)]
    colors: Vec<Vec2>,
    #[cfg(test)]
    normals: Vec<Vec2>,
}

pub(crate) struct AtlasIdxBundle {
    pub color: u32,
    pub normal: Option<u32>,
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

impl Registry for TextureRegistry {
    // GATs my beloved
    type Item<'a> = TextureRegistryEntry<'a>;

    fn get_by_label(&self, label: &ResourcePath) -> Option<Self::Item<'_>> {
        Some(self.get_by_id(self.get_id(label)?))
    }

    #[cfg(not(test))]
    fn get_by_id(&self, id: RegistryId<Self>) -> Self::Item<'_> {
        let map_idx = id.inner() as usize;
        let indices = self.map.get_index(map_idx).unwrap().1;

        TextureRegistryEntry {
            texture_idx: indices.color as u32,
            normal_idx: indices.normal.map(|v| v as u32),
            _data: PhantomData,
        }
    }

    #[cfg(test)]
    fn get_by_id(&self, id: RegistryId<Self>) -> Self::Item<'_> {
        let map_idx = id.inner() as usize;
        let indices = self.map.get_index(map_idx).unwrap().1;

        TextureRegistryEntry {
            texture_idx: indices.color as u32,
            normal_idx: indices.normal.map(|v| v as u32),
            _data: PhantomData,
        }
    }

    fn get_id(&self, label: &ResourcePath) -> Option<RegistryId<Self>> {
        self.map
            .get_index_of(label)
            .map(|idx| RegistryId::new(idx as _))
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
