mod ecs;
mod workers;

use std::{cmp, fmt};

use bevy::prelude::*;
use ecs::{batch_chunk_extraction, collect_solid_chunks_as_occluders, remove_chunks};
use workers::FinishedChunkMeshData;

use crate::{
    render::{
        lod::{LODs, LevelOfDetail, LodMap},
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

pub use self::ecs::{OccluderChunks, RemeshChunk};

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
    // TODO: maybe we should split this up into a per-lod thing.
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
        self.statuses[lod].set(pos, status);
    }

    /// Get the status of this chunk at different LODs.
    pub fn get_statuses(&self, pos: ChunkPos) -> LodMap<TimedChunkMeshStatus> {
        self.statuses
            .iter()
            .filter_map(|(lod, chunks)| chunks.get(pos).cloned().map(|status| (lod, status)))
            .collect::<LodMap<_>>()
    }

    /// "Flush" the queued mesh data. This marks it as ready for extraction so it will be extracted
    /// next time the extract schedule runs.
    pub fn flush(&mut self) {
        self.should_extract = true;
    }

    /// The number of chunks queued for extraction at this LOD.
    pub fn queued_additions(&self, lod: LevelOfDetail) -> usize {
        self.add[lod].len()
    }

    /// The number of chunks queued for removal from the render world at this LOD.
    pub fn queued_removals(&self, lod: LevelOfDetail) -> usize {
        self.remove[lod].len()
    }

    pub fn is_empty(&self, lod: LevelOfDetail) -> bool {
        self.queued_additions(lod) == 0 || self.queued_removals(lod) == 0
    }

    pub fn should_extract(&self) -> bool {
        self.should_extract
    }

    /// Try to queue a chunk mesh of a given age and LOD for extraction. Will do nothing if there's
    /// a newer version either already queued or extracted.
    pub fn add_chunk_mesh(&mut self, mesh: FinishedChunkMeshData) {
        // If we already have a newer chunk mesh, then we return early since we should never extract an
        // older version of a chunk mesh.
        if let Some(status) = self.statuses[mesh.lod].get(mesh.pos) {
            if status.tick > mesh.tick {
                return;
            }
        }

        // Will have an empty status if the mesh is empty
        let status = ChunkMeshStatus::from_mesh_data(&mesh.data);

        // Only queue the mesh for extraction if it's filled.
        if status == ChunkMeshStatus::Filled {
            self.add[mesh.lod].set(mesh.pos, mesh.data);
        }

        // Even if we don't queue the mesh for extraction we still need to note down its status.
        self.set_status(
            mesh.pos,
            mesh.lod,
            TimedChunkMeshStatus {
                tick: mesh.tick,
                status,
            },
        );
    }

    /// Queue a chunk at a given LOD for removal from the render world.
    pub fn remove_chunk(&mut self, pos: ChunkPos, lod: LevelOfDetail) {
        self.statuses[lod].remove(pos);
        self.add[lod].remove(pos);
        self.remove[lod].set(pos);
    }

    pub fn additions(
        &self,
        lod: LevelOfDetail,
    ) -> impl Iterator<Item = (ChunkPos, &ChunkMeshData)> + '_ {
        self.add[lod].iter()
    }

    pub fn removals(&self, lod: LevelOfDetail) -> impl Iterator<Item = ChunkPos> + '_ {
        self.remove[lod].iter()
    }

    /// Clear the removal and addition queues and mark the added chunks in the queue as being extracted.
    /// Also resets the 'Self::should_extract()' status.
    /// Should be called in the extract stage in the render world after copying data to communicate the
    /// status of the meshes to the main world.
    pub fn mark_as_extracted(&mut self, lods: LODs) {
        self.should_extract = false;

        for lod in lods.contained_lods() {
            self.remove[lod].clear();

            let additions = &mut self.add[lod];
            for (chunk, _) in additions.drain() {
                self.statuses[lod]
                    .get_mut(chunk)
                    .expect("All chunk positions queued for addition should have a status")
                    .status = ChunkMeshStatus::Extracted;
            }
        }
    }
}

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        info!("Initializing mesh controller");

        app.init_resource::<ExtractableChunkMeshData>()
            .init_resource::<OccluderChunks>()
            .add_event::<RemeshChunk>();

        app.add_systems(
            OnEnter(EngineState::Finished),
            setup_chunk_meshing_workers.in_set(CoreEngineSetup::Initialize),
        );

        app.add_systems(
            PreUpdate,
            (
                remove_chunks,
                insert_chunks,
                batch_chunk_extraction,
                collect_solid_chunks_as_occluders,
            )
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
