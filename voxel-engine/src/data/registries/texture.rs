use bevy::{
    asset::{AssetId, Handle},
    math::Vec2,
    render::texture::Image,
    sprite::{TextureAtlas, TextureAtlasBuilder},
};
use dashmap::DashMap;

use super::{Registry, RegistryId, RegistryStage};

pub struct TextureRegistry {
    map: DashMap<String, RegistryId<Self>, ahash::RandomState>,
    entries: Vec<TextureRegistryEntry>,
    atlas: TextureAtlas,
}

impl TextureRegistry {
    pub fn atlas_texture(&self) -> &Handle<Image> {
        &self.atlas.texture
    }
}

#[derive(Copy, Clone, Debug, dm::Constructor)]
pub struct TextureRegistryEntry {
    texture_pos: Vec2,
}

#[derive(Debug, dm::Constructor)]
pub struct RegistrableTexture {
    img: Image,
    id: AssetId<Image>,
}

impl Registry for TextureRegistry {
    type Item = TextureRegistryEntry;

    fn get_by_label(&self, label: &str) -> Option<&Self::Item> {
        todo!()
    }

    fn get_by_id(&self, id: RegistryId<Self>) -> &Self::Item {
        todo!()
    }

    fn get_id(&self, label: &str) -> RegistryId<Self> {
        todo!()
    }
}
