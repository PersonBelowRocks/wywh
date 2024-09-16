mod ecs;
pub mod events;
mod workers;

use std::{cmp, fmt, sync::Arc};

use async_bevy_events::{AsyncEventPlugin, EventFunnelPlugin};
use bevy::{
    prelude::*,
    tasks::{available_parallelism, TaskPoolBuilder},
};
use dashmap::DashMap;
use ecs::{
    batch_chunk_extraction, collect_solid_chunks_as_occluders,
    remove_chunk_meshes_from_extraction_bridge, send_mesh_removal_events_from_batch_removal_events,
};
use events::{
    BuildChunkMeshEvent, MeshFinishedEvent, RecalculateMeshBuildingEventPrioritiesEvent,
    RemoveChunkMeshEvent,
};
use workers::{
    start_mesh_builder_tasks, MESH_BUILDER_TASK_POOL, MESH_BUILDER_TASK_POOL_THREAD_NAME,
};

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

pub use self::ecs::OccluderChunks;

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

    /// Get the correct initial status of this mesh data. When a chunk mesh
    /// is queued for extraction it will have this status at first.
    ///
    /// Returns either:
    /// - [`ChunkMeshStatus::Empty`]
    /// - [`ChunkMeshStatus::Filled`]
    #[inline]
    pub fn status(&self) -> ChunkMeshStatus {
        if self.is_empty() {
            ChunkMeshStatus::Empty
        } else {
            ChunkMeshStatus::Filled
        }
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

/// The status of a chunk mesh, and the tick that the build event was sent at.
#[derive(Copy, Clone, Debug)]
pub struct TimedChunkMeshStatus {
    /// The tick that the build event for this mesh was sent on. This is not the same
    /// as the age of the chunk mesh, but it is always older than, or the same as, the chunk mesh's age.
    /// We keep track of this age so that the most up-to-date chunk mesh is used, and we want to ignore
    /// requests to remove chunk meshes if those requests are older than the chunk mesh.
    pub tick: u64,
    /// The status of the chunk mesh.
    pub status: ChunkMeshStatus,
}

/// Describes the status that a chunk mesh is in. This reflects the behaviour elsewhere in the engine
/// about how the chunk mesh should be treated.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChunkMeshStatus {
    /// The chunk mesh is not built, but it's queued to be built. Not all queued chunk
    /// meshes will have a status, so this status is not particularly useful if you need very strict logic.
    Unfulfilled,
    /// The chunk mesh is built, but is empty (i.e., has no geometry).
    /// This can happen if the chunk is only void or if the chunk is encased by solid blocks and
    /// thus all faces are culled. Empty chunk meshes will not be extracted
    /// but can overwrite a [filled][`ChunkMeshStatus::Filled`] mesh if the empty one is younger, or vice-versa.
    Empty,
    /// A filled chunk mesh has geometry and is ready to be [extracted][`ChunkMeshStatus::Extracted`] to the render world.
    Filled,
    /// The chunk mesh has been extracted into the render world, where we no longer have any say in how it's treated.
    /// Only filled meshes can be extracted. Extraction happens through the addition & removal buffers in [`ChunkMeshExtractBridge`].
    Extracted,
}

/// Describes the status of chunk meshes in the renderer. Setting the status of a chunk mesh
/// is only allowed to do through the [`ChunkMeshExtractBridge`] to ensure that the status
/// reflects the true location and behaviour of the chunk mesh.
pub struct ChunkMeshStatusManager {
    lods: LodMap<DashMap<ChunkPos, TimedChunkMeshStatus, rustc_hash::FxBuildHasher>>,
}

impl ChunkMeshStatusManager {
    pub fn new() -> Self {
        Self {
            lods: LodMap::from_fn(|_| {
                Some(DashMap::with_hasher(rustc_hash::FxBuildHasher::default()))
            }),
        }
    }

    /// Get the status and tick of a chunk mesh at the given LOD.
    /// Returns `None` if this chunk mesh does not exist at the given LOD.
    ///
    /// See [`TimedChunkMeshStatus`] for more information.
    #[inline]
    pub fn timed_status(
        &self,
        lod: LevelOfDetail,
        chunk_pos: ChunkPos,
    ) -> Option<TimedChunkMeshStatus> {
        self.lods[lod].get(&chunk_pos).as_deref().copied()
    }

    /// Returns `true` if this mesh status manager has a status for the given chunk at the given LOD.
    #[inline]
    pub fn contains(&self, lod: LevelOfDetail, chunk_pos: ChunkPos) -> bool {
        self.lods[lod].contains_key(&chunk_pos)
    }
}

/// Acts as a sort of bridge between the main world and render world for chunk meshes.
/// The render world will change its state depending on how this resource changes.
#[derive(Resource)]
pub struct ChunkMeshExtractBridge {
    statuses: Arc<ChunkMeshStatusManager>,
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

            statuses: Arc::new(ChunkMeshStatusManager::new()),
            add: LodMap::from_fn(|_| Some(ChunkMap::default())),
            remove: LodMap::from_fn(|_| Some(ChunkSet::default())),
        }
    }
}

impl ChunkMeshExtractBridge {
    fn set_status(
        &mut self,
        chunk_pos: ChunkPos,
        lod: LevelOfDetail,
        status: TimedChunkMeshStatus,
    ) {
        self.statuses.lods[lod].insert(chunk_pos, status);
    }

    /// Get the [`ChunkMeshStatusManager`] associated with this bridge.
    pub fn chunk_mesh_status_manager(&self) -> &Arc<ChunkMeshStatusManager> {
        &self.statuses
    }

    /// Get the status of this chunk at different LODs.
    pub fn get_statuses(&self, chunk_pos: ChunkPos) -> LodMap<TimedChunkMeshStatus> {
        self.statuses
            .lods
            .iter()
            .filter_map(|(lod, chunks)| {
                chunks
                    .get(&chunk_pos)
                    .as_deref()
                    .copied()
                    .map(|status| (lod, status))
            })
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
        let mut has_filled = false;

        // If we already have a newer chunk mesh, then we return early since we should never extract an
        // older version of a chunk mesh.
        if let Some(existing_status) = self.statuses.timed_status(lod, chunk_pos) {
            if existing_status.tick > tick {
                return;
            }

            has_filled = matches!(
                existing_status.status,
                ChunkMeshStatus::Filled | ChunkMeshStatus::Extracted
            );
        }

        let status = mesh_data.status();

        match status {
            // If the mesh is empty, queue it for removal so that the previous mesh (if it exists) is removed.
            ChunkMeshStatus::Empty if has_filled => {
                self.remove[lod].set(chunk_pos);
            }
            // Only queue the mesh for extraction if it's filled.
            ChunkMeshStatus::Filled => {
                self.add[lod].set(chunk_pos, mesh_data);
            }
            _ => (),
        }

        // Even if we don't queue the mesh for extraction we still need to note down its status.
        self.set_status(chunk_pos, lod, TimedChunkMeshStatus { tick, status });
    }

    /// Queue a chunk at a given LOD for removal from the render world.
    pub fn remove_chunk(&mut self, chunk_pos: ChunkPos, lod: LevelOfDetail, tick: u64) {
        if let Some(existing) = self.statuses.timed_status(lod, chunk_pos) {
            if existing.tick > tick {
                return;
            }
        }

        self.statuses.lods[lod].remove(&chunk_pos);
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
            for (chunk_pos, _) in additions.drain() {
                self.statuses.lods[lod]
                    .get_mut(&chunk_pos)
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

        MESH_BUILDER_TASK_POOL.set(
            TaskPoolBuilder::new()
                .num_threads(available_parallelism() / 2)
                .thread_name(MESH_BUILDER_TASK_POOL_THREAD_NAME.into())
                .build(),
        ).expect("build() should only be called once, and it's the only place where we initialize the pool");

        app.add_plugins((
            AsyncEventPlugin::<BuildChunkMeshEvent>::default(),
            AsyncEventPlugin::<RemoveChunkMeshEvent>::default(),
            AsyncEventPlugin::<RecalculateMeshBuildingEventPrioritiesEvent>::default(),
            EventFunnelPlugin::<MeshFinishedEvent>::for_new(),
        ))
        .init_resource::<ChunkMeshExtractBridge>()
        .init_resource::<OccluderChunks>();

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
