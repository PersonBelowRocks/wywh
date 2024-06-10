mod ecs;
mod workers;

use std::{cmp, fmt};

use bevy::prelude::*;
use ecs::{batch_chunk_extraction, remove_chunks};

use crate::{
    render::{meshing::controller::ecs::dispatch_updated_chunk_remeshings, quad::GpuQuad},
    util::{ChunkMap, ChunkSet},
    CoreEngineSetup, EngineState,
};

use self::ecs::{
    insert_chunks, queue_chunk_mesh_jobs, setup_chunk_meshing_workers,
    voxel_realm_remesh_updated_chunks,
};

pub use self::ecs::{MeshGeneration, RemeshChunk};

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

impl fmt::Debug for ChunkMeshData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();

        map.entry(&"indices", &self.index_buffer.len());
        map.entry(&"quads", &self.quad_buffer.len());

        map.finish()
    }
}

#[derive(Clone, Debug)]
pub struct TimedChunkMeshStatus {
    pub generation: u64,
    pub status: ChunkMeshStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChunkMeshStatus {
    Unfulfilled,
    Empty,
    Filled,
    Extracted,
}

impl ChunkMeshStatus {
    pub fn from_mesh_data(data: &ChunkMeshData) -> Self {
        if data.is_empty() {
            Self::Empty
        } else {
            Self::Filled
        }
    }
}

#[derive(Resource, Default)]
pub struct ExtractableChunkMeshData {
    pub active: ChunkMap<TimedChunkMeshStatus>,
    pub remove: ChunkSet,
    pub added: ChunkMap<ChunkMeshData>,
    /// Indicates if we should extract chunks to the render world (or remove chunks from the render world).
    /// Usually used to regulate extraction a bit so that we can extract chunks in bulk instead of extracting them immediately
    /// as they become available. This helps reduce some lag when meshing lots of chunks.
    pub should_extract: bool,
}

#[derive(Copy, Clone, PartialEq, dm::Constructor)]
pub struct ChunkRenderPermit {
    pub granted: u64,
}

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        info!("Setting up mesh controller");

        app.init_resource::<ExtractableChunkMeshData>()
            .init_resource::<MeshGeneration>()
            .add_event::<RemeshChunk>();

        app.add_systems(
            OnEnter(EngineState::Finished),
            setup_chunk_meshing_workers.after(CoreEngineSetup),
        );

        app.add_systems(
            PreUpdate,
            (remove_chunks, insert_chunks, batch_chunk_extraction)
                .chain()
                .run_if(in_state(EngineState::Finished)),
        );

        app.add_systems(
            FixedPostUpdate,
            (
                voxel_realm_remesh_updated_chunks.pipe(dispatch_updated_chunk_remeshings),
                queue_chunk_mesh_jobs,
            )
                .chain()
                .run_if(in_state(EngineState::Finished)),
        );
    }
}
