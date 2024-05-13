mod ecs;
mod workers;

use std::sync::Arc;

use bevy::prelude::*;
use dashmap::DashSet;

use crate::{
    render::quad::{ChunkQuads, GpuQuad},
    topo::world::ChunkPos,
    util::{ChunkMap, SyncChunkMap},
    AppState, CoreEngineSetup,
};

use self::ecs::{
    handle_incoming_permits, insert_chunks, queue_chunk_mesh_jobs, setup_chunk_meshing_workers,
    voxel_realm_remesh_updated_chunks,
};

pub use self::ecs::{ChunkMeshStorage, GrantPermit, MeshGeneration, RemeshChunk, RevokePermit};

#[derive(Clone)]
pub struct ChunkMeshData {
    pub index_buffer: Vec<u32>,
    pub quads: ChunkQuads,
}

#[derive(Clone)]
pub struct TimedChunkMeshData {
    pub generation: u64,
    pub data: ChunkMeshStatus,
}

#[derive(Clone)]
pub enum ChunkMeshStatus {
    Unfulfilled,
    Filled(ChunkMeshData),
    Extracted,
}

#[derive(Resource, Default)]
pub struct ExtractableChunkMeshData {
    pub map: ChunkMap<TimedChunkMeshData>,
}

#[derive(Copy, Clone, PartialEq, dm::Constructor)]
pub struct ChunkRenderPermit {
    pub granted: u64,
}

#[derive(Resource, Default)]
pub struct ChunkRenderPermits {
    pub(super) permits: ChunkMap<ChunkRenderPermit>,
}

impl ChunkRenderPermits {
    pub fn has_permit(&self, pos: ChunkPos) -> bool {
        self.permits.contains(pos)
    }

    pub fn revoke_permit(&mut self, pos: ChunkPos, generation: u64) {
        self.permits.remove(pos);
    }
}

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMeshStorage>()
            .init_resource::<ChunkRenderPermits>()
            .init_resource::<MeshGeneration>()
            .add_event::<RemeshChunk>()
            .add_event::<GrantPermit>()
            .add_event::<RevokePermit>();

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
            (
                handle_incoming_permits,
                voxel_realm_remesh_updated_chunks,
                queue_chunk_mesh_jobs,
            )
                .chain()
                .run_if(in_state(AppState::Finished)),
        );
    }
}
