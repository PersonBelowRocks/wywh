mod ecs;
pub mod events;
mod workers;

use std::{cmp, fmt};

use async_bevy_events::{AsyncEventPlugin, EventFunnelPlugin};
use bevy::prelude::*;
use ecs::{
    batch_chunk_extraction, collect_solid_chunks_as_occluders,
    remove_chunk_meshes_from_extraction_bridge, send_mesh_removal_events_from_batch_removal_events,
};
use events::{BuildMeshEvent, MeshFinishedEvent, RemoveChunkMeshEvent};
use workers::{start_mesh_builder_tasks, MeshBuilderTaskPool};

use crate::{
    render::{
        lod::{LODs, LevelOfDetail, LodMap},
        quad::GpuQuad,
    },
    topo::world::ChunkPos,
    util::{ChunkMap, ChunkSet},
    CoreEngineSetup, EngineState,
};

use self::ecs::prepare_finished_meshes_for_extraction;

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

/// Acts as a sort of bridge between the main world and render world for chunk meshes.
/// The render world will change its state depending on how this resource changes.
#[derive(Resource)]
pub struct ChunkMeshExtractBridge {
    statuses: LodMap<ChunkMap<TimedChunkMeshStatus>>,
    add: LodMap<ChunkMap<ChunkMeshData>>,
    remove: LodMap<ChunkSet>,
    /// Indicates if we should extract chunks to the render world (or remove chunks from the render world).
    /// Usually used to regulate extraction a bit so that we can extract chunks in bulk instead of extracting them immediately
    /// as they become available. This helps reduce some lag when meshing lots of chunks.
    // TODO: maybe we should split this up into a per-lod thing.
    should_extract: bool,
}

impl Default for ChunkMeshExtractBridge {
    fn default() -> Self {
        Self {
            should_extract: false,

            statuses: LodMap::from_fn(|_| Some(ChunkMap::default())),
            add: LodMap::from_fn(|_| Some(ChunkMap::default())),
            remove: LodMap::from_fn(|_| Some(ChunkSet::default())),
        }
    }
}

impl ChunkMeshExtractBridge {
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
    pub fn add_chunk_mesh(
        &mut self,
        chunk_pos: ChunkPos,
        lod: LevelOfDetail,
        tick: u64,
        mesh_data: ChunkMeshData,
    ) {
        // If we already have a newer chunk mesh, then we return early since we should never extract an
        // older version of a chunk mesh.
        if let Some(status) = self.statuses[lod].get(chunk_pos) {
            if status.tick > tick {
                return;
            }
        }

        // Will have an empty status if the mesh is empty
        let status = ChunkMeshStatus::from_mesh_data(&mesh_data);

        // Only queue the mesh for extraction if it's filled.
        if status == ChunkMeshStatus::Filled {
            self.add[lod].set(chunk_pos, mesh_data);
        }

        // Even if we don't queue the mesh for extraction we still need to note down its status.
        self.set_status(chunk_pos, lod, TimedChunkMeshStatus { tick, status });
    }

    /// Queue a chunk at a given LOD for removal from the render world.
    pub fn remove_chunk(&mut self, chunk_pos: ChunkPos, lod: LevelOfDetail, tick: u64) {
        if let Some(existing) = self.statuses[lod].get(chunk_pos) {
            if existing.tick > tick {
                return;
            }
        }

        self.statuses[lod].remove(chunk_pos);
        self.add[lod].remove(chunk_pos);
        self.remove[lod].set(chunk_pos);
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

        let mesh_builder_task_pool = MeshBuilderTaskPool::default();

        app.add_plugins((
            AsyncEventPlugin::<BuildMeshEvent>::default(),
            EventFunnelPlugin::<MeshFinishedEvent>::for_new(),
            AsyncEventPlugin::<RemoveChunkMeshEvent>::default(),
        ))
        .insert_resource(mesh_builder_task_pool)
        .init_resource::<ChunkMeshExtractBridge>()
        .init_resource::<OccluderChunks>()
        .add_event::<RemeshChunk>();

        app.add_systems(
            OnEnter(EngineState::Finished),
            start_mesh_builder_tasks.in_set(CoreEngineSetup::Initialize),
        );

        app.add_systems(
            PreUpdate,
            (
                // TODO: send mesh building events when necessary!
                send_mesh_removal_events_from_batch_removal_events,
                remove_chunk_meshes_from_extraction_bridge,
                prepare_finished_meshes_for_extraction,
                batch_chunk_extraction,
                collect_solid_chunks_as_occluders,
            )
                .chain()
                .run_if(in_state(EngineState::Finished)),
        );
    }
}
