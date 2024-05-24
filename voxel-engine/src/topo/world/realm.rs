use std::sync::Arc;

use bevy::{
    ecs::{
        entity::Entity,
        system::{Res, SystemParam},
    },
    math::{ivec3, IVec3},
    prelude::Resource,
};
use dashmap::{mapref::one::Ref, DashSet};

use crate::{
    topo::{
        block::{BlockVoxel, FullBlock},
        controller::{ChunkEcsPermits, ChunkPermitKey, PermitFlags},
        neighbors::{Neighbors, NEIGHBOR_ARRAY_SIZE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS},
        world::chunk::ChunkFlags,
    },
    util::{ivec3_to_1d, SyncHashMap},
};

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
            .is_some_and(|permit| permit.flags.contains(PermitFlags::RENDER))
    }
}
