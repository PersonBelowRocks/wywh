use bevy::{
    asset::AssetId,
    math::Vec2,
    render::texture::Image,
    sprite::{TextureAtlas, TextureAtlasBuilder},
};
use dashmap::DashMap;

use super::{Registry, RegistryId, RegistryStage};

pub struct TextureRegistry {
    map: DashMap<String, RegistryId<Self>, ahash::RandomState>,
    entries: Vec<TextureRegistryEntry>,
    atlas: RegistryStage<TextureAtlasBuilder, TextureAtlas>,
}

impl TextureRegistry {
    pub fn new() -> Self {
        Self {
            map: DashMap::with_hasher(ahash::RandomState::new()),
            entries: Vec::new(),
            atlas: RegistryStage::Loading(TextureAtlasBuilder::default()),
        }
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
    type ItemIn = RegistrableTexture;
    type ItemOut = TextureRegistryEntry;

    fn register(&mut self, label: &str, entry: Self::ItemIn) -> RegistryId<Self> {
        let RegistryStage::Loading(builder) = &mut self.atlas else {
            panic!("Cannot add entry to frozen registry")
        };

        builder.add_texture(entry.id, &entry.img);

        todo!()
    }

    fn freeze(&mut self) {
        todo!()
    }

    fn is_frozen(&self) -> bool {
        todo!()
    }

    fn get_by_label(&self, label: &str) -> Option<&Self::ItemOut> {
        todo!()
    }

    fn get_by_id(&self, id: RegistryId<Self>) -> &Self::ItemOut {
        todo!()
    }

    fn get_id(&self, label: &str) -> RegistryId<Self> {
        todo!()
    }
}
