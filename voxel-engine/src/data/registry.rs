use bevy::prelude::*;

use crate::util::SyncHashMap;

use super::tile::{Transparency, VoxelId, VoxelTexture};

#[derive(Clone)]
pub struct VoxelProperties {
    transparency: Transparency,
    texture_id: TextureId,
}

pub struct VoxelRegistry {
    map: hb::HashMap<VoxelId, VoxelProperties>,
}

impl VoxelRegistry {
    pub fn new() -> Self {
        Self {
            map: Default::default(),
        }
    }
    // pub fn register(&mut self)
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, dm::From, dm::Into)]
pub struct TextureId(u32);

impl TextureId {
    pub fn new(d: u32) -> Self {
        Self(d)
    }

    pub fn inner(self) -> u32 {
        self.0
    }
}

pub struct VoxelTextureRegistry {
    texture_atlas_uvs: SyncHashMap<TextureId, Rect>,
    atlas: TextureAtlas,
    asset_server: AssetServer,
}

impl VoxelTextureRegistry {
    pub fn new(asset_server: AssetServer) -> Self {
        Self {
            texture_atlas_uvs: default(),
            atlas: todo!(),
            asset_server,
        }
    }
}
