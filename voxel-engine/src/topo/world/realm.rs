use std::sync::Arc;

use bevy::{
    ecs::system::{Res, SystemParam},
    prelude::Query,
};

use crate::topo::controller::VoxelWorldTick;

use super::{
    ChunkPos,
    chunk_manager::{ChunkManager, ecs::ChunkManagerRes},
};

#[derive(SystemParam)]
pub struct VoxelRealm<'w, 's> {
    chunk_manager: Res<'w, ChunkManagerRes>,
    tick: Res<'w, VoxelWorldTick>,
}

impl<'w, 's> VoxelRealm<'w, 's> {
    pub fn tick(&self) -> u64 {
        self.tick.get()
    }

    pub fn cm(&self) -> &ChunkManager {
        self.chunk_manager.0.as_ref()
    }

    pub fn clone_cm(&self) -> Arc<ChunkManager> {
        self.chunk_manager.0.clone()
    }
}
