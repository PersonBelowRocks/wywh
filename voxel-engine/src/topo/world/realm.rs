use std::sync::Arc;

use bevy::{
    ecs::system::{Res, SystemParam},
    prelude::Resource,
};

use crate::topo::controller::{ChunkEcsPermits, ChunkPermitKey, PermitFlags};

use super::{chunk_manager::ChunkManager, ChunkPos};

#[derive(Resource)]
pub struct ChunkManagerResource(pub(crate) Arc<ChunkManager>);

#[derive(SystemParam)]
pub struct VoxelRealm<'w> {
    chunk_manager: Res<'w, ChunkManagerResource>,
    permits: Res<'w, ChunkEcsPermits>,
}

impl<'w> VoxelRealm<'w> {
    pub fn cm(&self) -> &ChunkManager {
        self.chunk_manager.0.as_ref()
    }

    pub fn clone_cm(&self) -> Arc<ChunkManager> {
        self.chunk_manager.0.clone()
    }

    pub fn permits(&self) -> &ChunkEcsPermits {
        &self.permits
    }

    pub fn has_render_permit(&self, pos: ChunkPos) -> bool {
        self.permits()
            .get(ChunkPermitKey::Chunk(pos))
            .is_some_and(|permit| permit.cached_flags.contains(PermitFlags::RENDER))
    }
}
