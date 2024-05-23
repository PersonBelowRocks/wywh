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
        neighbors::{Neighbors, NEIGHBOR_ARRAY_SIZE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS},
        world::chunk::ChunkFlags,
    },
    util::{ivec3_to_1d, SyncHashMap},
};

use super::{
    chunk::{Chunk, ChunkPos},
    chunk_entity::CEBimap,
    chunk_manager::ChunkManager,
    chunk_ref::{ChunkRef, ChunkRefReadAccess},
    error::ChunkManagerError,
};

#[derive(Resource)]
pub struct ChunkManagerResource(pub(crate) Arc<ChunkManager>);

#[derive(Resource)]
pub struct ChunkEntitiesBijectionResource(pub(crate) CEBimap);

#[derive(SystemParam)]
pub struct VoxelRealm<'w> {
    chunk_manager: Res<'w, ChunkManagerResource>,
    chunk_entities: Res<'w, ChunkEntitiesBijectionResource>,
}

impl<'w> VoxelRealm<'w> {
    pub fn cm(&self) -> &ChunkManager {
        self.chunk_manager.0.as_ref()
    }

    pub fn clone_cm(&self) -> Arc<ChunkManager> {
        self.chunk_manager.0.clone()
    }

    pub fn ce_bimap(&self) -> &CEBimap {
        &self.chunk_entities.0
    }
}
