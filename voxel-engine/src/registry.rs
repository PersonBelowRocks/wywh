use bevy::prelude::*;

use crate::tile::{TextureId, VoxelTexture};

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
