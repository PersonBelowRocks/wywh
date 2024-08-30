use std::sync::Arc;

use bevy::{
    ecs::system::{Res, SystemParam},
    prelude::{Query, Resource},
};

use crate::topo::controller::{
    BatchFlags, CachedBatchMembership, ChunkBatch, LoadshareId, LoadshareProvider, VoxelWorldTick,
};

use super::{
    chunk_manager::{ecs::ChunkManagerRes, ChunkManager},
    ChunkPos,
};

#[derive(SystemParam)]
pub struct VoxelRealm<'w, 's> {
    chunk_manager: Res<'w, ChunkManagerRes>,
    membership: Res<'w, CachedBatchMembership>,
    loadshares: Res<'w, LoadshareProvider>,
    tick: Res<'w, VoxelWorldTick>,
    q_batches: Query<'w, 's, &'static ChunkBatch>,
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

    pub fn has_render_permit(&self, pos: ChunkPos) -> bool {
        let Some(membership) = self.membership.get(pos) else {
            return false;
        };

        for &batch_entity in membership.iter() {
            let batch = self.q_batches.get(batch_entity).unwrap();

            if batch.flags().contains(BatchFlags::RENDER) {
                return true;
            }
        }

        false
    }

    pub fn has_loadshare(&self, loadshare: LoadshareId) -> bool {
        self.loadshares.contains(loadshare)
    }
}
