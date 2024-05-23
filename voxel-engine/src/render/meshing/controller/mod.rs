mod ecs;
mod workers;

use std::cmp;

use bevy::prelude::*;

use crate::{
    render::{meshing::controller::ecs::dispatch_updated_chunk_remeshings, quad::GpuQuad},
    topo::world::ChunkPos,
    util::ChunkMap,
    AppState, CoreEngineSetup,
};

use self::ecs::{
    handle_incoming_permits, insert_chunks, queue_chunk_mesh_jobs, setup_chunk_meshing_workers,
    voxel_realm_remesh_updated_chunks,
};

pub use self::ecs::{GrantPermit, MeshGeneration, RemeshChunk, RevokePermit};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RemeshType {
    Immediate,
    Delayed,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Ord)]
pub struct RemeshPriority(u32);

impl RemeshPriority {
    pub const HIGHEST: Self = Self(0);
    pub const LOWEST: Self = Self(u32::MAX);

    pub fn new(raw: u32) -> Self {
        Self(raw)
    }
}

impl PartialOrd for RemeshPriority {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        other.0.partial_cmp(&self.0)
    }
}

#[derive(Clone)]
pub struct ChunkMeshData {
    pub index_buffer: Vec<u32>,
    pub quad_buffer: Vec<GpuQuad>,
}

impl ChunkMeshData {
    pub fn is_empty(&self) -> bool {
        self.index_buffer.is_empty() || self.quad_buffer.is_empty()
    }
}

#[derive(Clone)]
pub struct TimedChunkMeshData {
    pub generation: u64,
    pub data: ChunkMeshStatus,
}

#[derive(Clone)]
pub enum ChunkMeshStatus {
    Unfulfilled,
    Empty,
    Filled(ChunkMeshData),
    Extracted,
}

impl ChunkMeshStatus {
    pub fn from_mesh_data(data: &ChunkMeshData) -> Self {
        if data.is_empty() {
            Self::Empty
        } else {
            Self::Filled(data.clone())
        }
    }
}

#[derive(Resource, Default)]
pub struct ExtractableChunkMeshData {
    pub active: ChunkMap<TimedChunkMeshData>,
    pub removed: Vec<ChunkPos>,
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

    pub fn revoke_permit(&mut self, pos: ChunkPos, _generation: u64) {
        self.permits.remove(pos);
    }
}

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        info!("Setting up mesh controller");

        app.init_resource::<ChunkRenderPermits>()
            .init_resource::<ExtractableChunkMeshData>()
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
            FixedPostUpdate,
            (
                handle_incoming_permits,
                voxel_realm_remesh_updated_chunks.pipe(dispatch_updated_chunk_remeshings),
                queue_chunk_mesh_jobs,
            )
                .chain()
                .run_if(in_state(AppState::Finished)),
        );
    }
}
