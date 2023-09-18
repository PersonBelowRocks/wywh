use bevy::prelude::*;

use super::tile::{TextureId, Transparency, VoxelId, VoxelTexture};

pub struct VoxelProperties {
    transparency: Transparency,
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

pub struct TextureRegistry {
    textures: hb::HashMap<TextureId, VoxelTexture>,
    asset_server: AssetServer,
}

impl TextureRegistry {
    pub fn new(asset_server: AssetServer) -> Self {
        Self {
            textures: default(),
            asset_server,
        }
    }
}
