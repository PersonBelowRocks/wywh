mod ecs;
mod workers;

use std::sync::Arc;

use bevy::prelude::*;
use dashmap::DashSet;

use crate::{
    render::quad::{ChunkQuads, GpuQuad},
    topo::world::ChunkPos,
    util::SyncChunkMap,
    AppState, CoreEngineSetup,
};

use self::ecs::{
    insert_chunks, queue_chunk_mesh_jobs, remesh_chunks, setup_chunk_meshing_workers,
    ChunkMeshStorage, RemeshChunk,
};

#[derive(Clone)]
pub struct ChunkMeshData {
    pub index_buffer: Vec<u32>,
    pub quads: ChunkQuads,
}

#[derive(Clone)]
pub struct TimedChunkMeshData {
    pub generation: u64,
    pub data: ChunkMeshData,
}

#[derive(Resource, Default)]
pub struct ChunkRenderPermits {
    pub(super) new_permits: DashSet<ChunkPos, fxhash::FxBuildHasher>,
    pub(super) revoked_permits: DashSet<ChunkPos, fxhash::FxBuildHasher>,
    pub(super) filled_permits: Arc<SyncChunkMap<TimedChunkMeshData>>,
}

impl ChunkRenderPermits {
    pub fn grant_permit(&self, pos: ChunkPos) {
        self.new_permits.insert(pos);
    }

    pub fn has_permit(&self, pos: ChunkPos) -> bool {
        (self.filled_permits.contains(pos) || self.new_permits.contains(&pos))
            && !self.revoked_permits.contains(&pos)
    }

    pub fn revoke_permit(&self, pos: ChunkPos) {
        self.revoked_permits.remove(&pos);
    }

    pub fn filled_permit_map(&self) -> &Arc<SyncChunkMap<TimedChunkMeshData>> {
        &self.filled_permits
    }
}

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMeshStorage>()
            .add_event::<RemeshChunk>();

        app.add_systems(
            OnEnter(AppState::Finished),
            setup_chunk_meshing_workers.after(CoreEngineSetup),
        );

        app.add_systems(
            PreUpdate,
            insert_chunks.run_if(in_state(AppState::Finished)),
        );

        app.add_systems(
            PostUpdate,
            (remesh_chunks, queue_chunk_mesh_jobs)
                .chain()
                .run_if(in_state(AppState::Finished)),
        );
    }
}
