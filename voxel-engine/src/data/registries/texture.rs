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

use crate::data::{resourcepath::ResourcePath, texture::GpuFaceTexture};

use super::{error::TextureRegistryError, Registry, RegistryId};

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
        textures: &mut Assets<Image>,
    ) -> Result<TextureRegistry, TextureRegistryError> {
        let color_atlas = {
            let mut builder = TextureAtlasBuilder::default();
            for id in self.textures.values().cloned() {
                let tex = textures
                    .get(id.color)
                    .ok_or(TextureRegistryError::TextureNotLoaded(id.color))?;
                builder.add_texture(id.color, tex);
            }

            builder.finish(textures)?
        };

        let normal_atlas = {
            let mut builder = TextureAtlasBuilder::default();
            for id in self.textures.values().cloned() {
                let Some(normal_map) = id.normal else {
                    continue;
                };

                let tex = textures
                    .get(normal_map)
                    .ok_or(TextureRegistryError::TextureNotLoaded(normal_map))?;
                builder.add_texture(normal_map, tex);
            }

            builder.finish(textures)?
        };

        for (label, id) in self.textures.iter() {
            let idx = color_atlas.get_texture_index(id.color.clone()).unwrap();
            let rect = color_atlas.textures[idx];
            info!(
                "Texture registry contains texture '{label}' at {}",
                rect.min
            );

            if let Some(normal_id) = id.normal {
                let idx = normal_atlas.get_texture_index(normal_id).unwrap();
                let pos = normal_atlas.textures[idx].min;

                info!(
                    "Texture '{label}' has a normal map at position {pos} in the normal map atlas."
                )
            }
        }

        let registry_map = {
            let mut map =
                IndexMap::<ResourcePath, AtlasIdxBundle, ahash::RandomState>::with_capacity_and_hasher(
                    self.textures.len(),
                    ahash::RandomState::new(),
                );

            map.reserve(self.textures.len());
            for (label, ids) in self.textures.into_iter() {
                let indices = AtlasIdxBundle {
                    color: color_atlas.get_texture_index(ids.color).unwrap(),
                    normal: ids
                        .normal
                        .map(|id| normal_atlas.get_texture_index(id).unwrap()),
                };

                map.insert(label, indices);
            }

            map
        };

        Ok(TextureRegistry {
            map: registry_map,
            color_atlas,
            normal_atlas,

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
            color: self.colors.len(),
            normal: normal.map(|_| self.normals.len()),
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

            color_atlas: TextureAtlas::new_empty(Handle::weak_from_u128(0), Vec2::ONE),
            normal_atlas: TextureAtlas::new_empty(Handle::weak_from_u128(1), Vec2::ONE),

            colors: self.colors,
            normals: self.normals,
        }
    }
}

#[derive(Clone, Resource)]
pub struct TexregFaces(pub Vec<GpuFaceTexture>);

pub struct TextureRegistry {
    map: IndexMap<ResourcePath, AtlasIdxBundle, ahash::RandomState>,

    color_atlas: TextureAtlas,
    normal_atlas: TextureAtlas,

    #[cfg(test)]
    colors: Vec<Vec2>,
    #[cfg(test)]
    normals: Vec<Vec2>,
}

pub(crate) struct AtlasIdxBundle {
    pub color: usize,
    pub normal: Option<usize>,
}

impl TextureRegistry {
    pub fn color_texture(&self) -> &Handle<Image> {
        &self.color_atlas.texture
    }

    pub fn normal_texture(&self) -> &Handle<Image> {
        &self.normal_atlas.texture
    }

    pub fn texture_scale(&self) -> f32 {
        // TODO: this should be configurable without recompiling so we can support textures of different resolutions
        16.0
    }

    pub fn face_texture_buffer(&self) -> Vec<GpuFaceTexture> {
        self.map
            .values()
            .map(|indices| {
                let color_pos = self.color_atlas.textures[indices.color].min;
                let normal_pos = indices
                    .normal
                    .map(|idx| self.normal_atlas.textures[idx].min);

                GpuFaceTexture::new(color_pos, normal_pos)
            })
            .collect::<Vec<_>>()
    }
}

#[derive(Copy, Clone, Debug, dm::Constructor)]
pub struct TextureRegistryEntry<'a> {
    pub texture_pos: Vec2,
    pub normal_pos: Option<Vec2>,

    // Placeholder in case we wanna store some other funny stuff in here
    _data: PhantomData<&'a ()>,
}

impl<'a> TextureRegistryEntry<'a> {
    pub fn gpu_representation(&self) -> GpuFaceTexture {
        GpuFaceTexture::new(self.texture_pos, self.normal_pos)
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
            texture_pos: self.color_atlas.textures[indices.color].min,
            normal_pos: indices
                .normal
                .map(|idx| self.normal_atlas.textures[idx].min),
            _data: PhantomData,
        }
    }

    #[cfg(test)]
    fn get_by_id(&self, id: RegistryId<Self>) -> Self::Item<'_> {
        let map_idx = id.inner() as usize;
        let indices = self.map.get_index(map_idx).unwrap().1;

        TextureRegistryEntry {
            texture_pos: self.colors[indices.color],
            normal_pos: indices.normal.map(|idx| self.normals[idx]),
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
