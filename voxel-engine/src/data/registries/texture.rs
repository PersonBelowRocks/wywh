use std::marker::PhantomData;

use bevy::{
    asset::{AssetId, Assets, Handle},
    math::Vec2,
    render::texture::Image,
    sprite::{TextureAtlas, TextureAtlasBuilder, TextureAtlasBuilderError},
};

use super::{Registry, RegistryId};

pub struct TextureRegistryLoader {
    map: indexmap::IndexMap<String, AssetId<Image>, ahash::RandomState>,
    builder: TextureAtlasBuilder,
}

impl TextureRegistryLoader {
    pub fn new() -> Self {
        Self {
            map: indexmap::IndexMap::with_hasher(ahash::RandomState::new()),
            builder: TextureAtlasBuilder::default(),
        }
    }

    pub fn register(&mut self, label: impl Into<String>, id: AssetId<Image>, tex: &Image) {
        self.builder.add_texture(id, tex);
        self.map.insert(label.into(), id);
    }

    pub fn build_registry(
        self,
        textures: &mut Assets<Image>,
    ) -> Result<TextureRegistry, TextureAtlasBuilderError> {
        let mut registry_map =
            hb::HashMap::<String, RegistryId<TextureRegistry>, ahash::RandomState>::with_capacity_and_hasher(
                self.map.len(),
                ahash::RandomState::new(),
            );
        let atlas = self.builder.finish(textures)?;

        registry_map.extend(
            self.map
                .into_iter()
                .map(|(lbl, id)| (lbl, atlas.get_texture_index(id).unwrap()))
                .map(|(lbl, index)| (lbl, RegistryId::<TextureRegistry>::new(index as _))),
        );

        Ok(TextureRegistry {
            map: registry_map,
            atlas,
        })
    }
}

pub struct TextureRegistry {
    map: hb::HashMap<String, RegistryId<Self>, ahash::RandomState>,
    atlas: TextureAtlas,
}

impl TextureRegistry {
    pub fn atlas_texture(&self) -> &Handle<Image> {
        &self.atlas.texture
    }

    pub fn texture_scale(&self) -> f32 {
        // TODO: this should be configurable without recompiling so we can support textures of different resolutions
        16.0
    }
}

#[derive(Copy, Clone, Debug, dm::Constructor)]
pub struct TextureRegistryEntry<'a> {
    pub texture_pos: Vec2,

    // Placeholder in case we wanna store some other funny stuff in here
    _data: PhantomData<&'a ()>,
}

impl Registry for TextureRegistry {
    // GATs my beloved
    type Item<'a> = TextureRegistryEntry<'a>;

    fn get_by_label(&self, label: &str) -> Option<Self::Item<'_>> {
        Some(self.get_by_id(self.get_id(label)?))
    }

    fn get_by_id(&self, id: RegistryId<Self>) -> Self::Item<'_> {
        let idx = id.inner() as usize;
        TextureRegistryEntry {
            texture_pos: self.atlas.textures[idx].min,
            _data: PhantomData,
        }
    }

    fn get_id(&self, label: &str) -> Option<RegistryId<Self>> {
        self.map.get(label).copied()
    }
}

#[cfg(test)]
mod tests {
    use bevy::utils::Uuid;

    use super::*;

    // this is just a compile time test to make sure lifetimes and everything work out
    fn texture_registry_loading() {
        let loader = TextureRegistryLoader::new();
        let registry = loader.build_registry(todo!()).unwrap();
        let tex = registry.get_by_label("wowza!").unwrap();
    }

    #[test]
    #[ignore]
    fn texture_registry_basics() {
        todo!()
    }
}
