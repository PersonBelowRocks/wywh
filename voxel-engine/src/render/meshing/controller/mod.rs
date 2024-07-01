mod ecs;
mod workers;

use std::{cmp, fmt};

use bevy::prelude::*;
use ecs::{batch_chunk_extraction, remove_chunks};

use crate::{
    render::{
        lod::{LevelOfDetail, LodMap},
        meshing::controller::ecs::dispatch_updated_chunk_remeshings,
        quad::GpuQuad,
    },
    topo::world::ChunkPos,
    util::{ChunkMap, ChunkSet},
    CoreEngineSetup, EngineState,
};

use self::ecs::{
    insert_chunks, queue_chunk_mesh_jobs, setup_chunk_meshing_workers,
    voxel_realm_remesh_updated_chunks,
};

pub use self::ecs::RemeshChunk;

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

// TODO: a ChunkMesh type that contains the mesh data, chunk position, and LOD
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

#[derive(Copy, Clone, Debug)]
pub struct TimedChunkMeshStatus {
    pub tick: u64,
    pub status: ChunkMeshStatus,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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

#[derive(Resource)]
pub struct ExtractableChunkMeshData {
    statuses: LodMap<ChunkMap<TimedChunkMeshStatus>>,
    add: LodMap<ChunkMap<ChunkMeshData>>,
    remove: LodMap<ChunkSet>,
    /// Indicates if we should extract chunks to the render world (or remove chunks from the render world).
    /// Usually used to regulate extraction a bit so that we can extract chunks in bulk instead of extracting them immediately
    /// as they become available. This helps reduce some lag when meshing lots of chunks.
    should_extract: bool,
}

impl Default for ExtractableChunkMeshData {
    fn default() -> Self {
        Self {
            should_extract: false,

            statuses: LodMap::from_fn(|_| Some(ChunkMap::default())),
            add: LodMap::from_fn(|_| Some(ChunkMap::default())),
            remove: LodMap::from_fn(|_| Some(ChunkSet::default())),
        }
    }
}

impl ExtractableChunkMeshData {
    fn set_status(&mut self, pos: ChunkPos, lod: LevelOfDetail, status: TimedChunkMeshStatus) {
        self.statuses.get_mut(lod).unwrap().set(pos, status);
    }

    /// "Flush" the queued mesh data. This marks it as ready for extraction so it will be extracted
    /// next time the extract schedule runs.
    pub fn flush(&mut self) {
        self.should_extract = true;
    }

    /// The number of chunks queued for extraction at this LOD.
    pub fn queued_additions(&self, lod: LevelOfDetail) -> usize {
        self.add.get(lod).unwrap().len()
    }

    /// The number of chunks queued for removal from the render world at this LOD.
    pub fn queued_removals(&self, lod: LevelOfDetail) -> usize {
        self.remove.get(lod).unwrap().len()
    }

    pub fn should_extract(&self) -> bool {
        self.should_extract
    }

    /// Try to queue a chunk mesh of a given age and LOD for extraction. Will do nothing if there's
    /// a newer version either already queued or extracted.
    pub fn add_chunk_mesh(
        &mut self,
        pos: ChunkPos,
        lod: LevelOfDetail,
        tick: u64,
        mesh: ChunkMeshData,
    ) {
        // If we already have a newer chunk mesh, then we return early since we should never extract an
        // older version of a chunk mesh.
        if let Some(status) = self.statuses.get(lod).unwrap().get(pos) {
            if status.tick > tick {
                return;
            }
        }

        // Will have an empty status if the mesh is empty
        let status = ChunkMeshStatus::from_mesh_data(&mesh);

        // Only queue the mesh for extraction if it's filled.
        if status == ChunkMeshStatus::Filled {
            self.add.get_mut(lod).unwrap().set(pos, mesh);
        }

        // Even if we don't queue the mesh for extraction we still need to note down its status.
        self.set_status(pos, lod, TimedChunkMeshStatus { tick, status });
    }

    /// Queue a chunk at a given LOD for removal from the render world.
    pub fn remove_chunk(&mut self, pos: ChunkPos, lod: LevelOfDetail) {
        self.statuses.get_mut(lod).unwrap().remove(pos);
        self.remove.get_mut(lod).unwrap().remove(pos);
    }

    pub fn additions(
        &self,
        lod: LevelOfDetail,
    ) -> impl Iterator<Item = (ChunkPos, &ChunkMeshData)> + '_ {
        self.add.get(lod).unwrap().iter()
    }

    pub fn removals(&self, lod: LevelOfDetail) -> impl Iterator<Item = ChunkPos> + '_ {
        self.remove.get(lod).unwrap().iter()
    }

    pub fn clear(&mut self) {
        self.should_extract = false;
        self.remove.clear();
        self.add.clear();
    }
}

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        info!("Setting up mesh controller");

        app.init_resource::<ExtractableChunkMeshData>()
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
