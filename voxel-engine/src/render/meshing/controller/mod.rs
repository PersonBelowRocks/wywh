mod ecs;
pub mod events;
mod scheduler;
mod state_tracking;

use async_bevy_events::{AsyncEventPlugin, EventFunnelPlugin};
use bevy::tasks::TaskPool;
use bevy::{
    prelude::*,
    tasks::{available_parallelism, TaskPoolBuilder},
};
use dashmap::DashMap;
use ecs::{
    batch_chunk_extraction, collect_solid_chunks_as_occluders,
    remove_chunk_meshes_from_extraction_bridge, send_mesh_removal_events_from_batch_removal_events,
};
use events::{BuildChunkMeshEvent, MeshFinishedEvent, RemoveChunkMeshEvent};
use std::sync::OnceLock;
use std::{
    cmp::{self, max},
    sync::Arc,
};

use self::ecs::prepare_finished_meshes_for_extraction;
use crate::render::meshing::controller::state_tracking::{
    ChunkMeshExtractBridge, ChunkMeshState, ChunkMeshTimestate,
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

pub use self::ecs::OccluderChunks;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RemeshType {
    Immediate,
    Delayed,
}

/// The number of [`GpuQuad`]s in a byte buffer of the given size.
pub fn quads_in_byte_buffer(buffer_size: u64) -> u32 {
    (buffer_size / GpuQuad::ARRAY_STRIDE).try_into().unwrap()
}

/// The number of `u32` values in a byte buffer of the given size.
pub fn u32s_in_byte_buffer(buffer_size: u64) -> u32 {
    use bevy::render::render_resource::encase::ShaderSize;
    (buffer_size / u32::SHADER_SIZE.get()).try_into().unwrap()
}

// TODO: a ChunkMesh type that contains the mesh data, chunk position, and LOD
/// Data for a chunk mesh, formatted to be uploaded to the GPU.
#[derive(Clone)]
pub struct ChunkMeshData {
    /// The index buffer of this chunk, binary format depends on the LOD of the mesh.
    pub index_buffer_data: Vec<u8>,
    /// The quad buffer of this chunk, binary format depends on the LOD of the mesh.
    pub quad_buffer_data: Vec<u8>,
    /// The LOD of the chunk mesh, determines the format of the data buffers.
    pub lod: LevelOfDetail,
}

impl ChunkMeshData {
    /// Get an empty chunk mesh
    pub fn empty(lod: LevelOfDetail) -> Self {
        Self {
            index_buffer_data: Vec::new(),
            quad_buffer_data: Vec::new(),
            lod,
        }
    }

    /// The number of indices in this mesh.
    #[inline]
    pub fn indices(&self) -> u32 {
        u32s_in_byte_buffer(self.index_buffer_data.len() as u64)
    }

    /// The number of quads in this mesh.
    #[inline]
    pub fn quads(&self) -> u32 {
        quads_in_byte_buffer(self.quad_buffer_data.len() as u64)
    }

    pub fn is_empty(&self) -> bool {
        self.index_buffer_data.is_empty() || self.quad_buffer_data.is_empty()
    }

    /// Get the correct initial status of this mesh data. When a chunk mesh
    /// is queued for extraction it will have this status at first.
    ///
    /// Returns either:
    /// - [`ChunkMeshState::Empty`]
    /// - [`ChunkMeshState::Filled`]
    #[inline]
    pub fn status(&self) -> ChunkMeshState {
        if self.is_empty() {
            ChunkMeshState::Empty
        } else {
            ChunkMeshState::Filled
        }
    }
}

// /// Acts as a sort of bridge between the main world and render world for chunk meshes.
// /// The render world will change its state depending on how this resource changes.
// #[derive(Resource)]
// pub struct ChunkMeshExtractBridge {
//     statuses: Arc<ChunkMeshStatusManager>,
//     add: LodMap<ChunkMap<ChunkMeshData>>,
//     remove: LodMap<ChunkSet>,
//     /// Indicates if we should extract chunks to the render world (or remove chunks from the render world).
//     /// Usually used to regulate extraction a bit so that we can extract chunks in bulk instead of extracting them immediately
//     /// as they become available. This helps reduce some lag when meshing lots of chunks.
//     // TODO: maybe we should split this up into a per-lod thing.
//     should_extract: bool,
// }
//
// impl Default for ChunkMeshExtractBridge {
//     fn default() -> Self {
//         Self {
//             should_extract: false,
//
//             statuses: Arc::new(ChunkMeshStatusManager::new()),
//             add: LodMap::from_fn(|_| Some(ChunkMap::default())),
//             remove: LodMap::from_fn(|_| Some(ChunkSet::default())),
//         }
//     }
// }
//
// impl ChunkMeshExtractBridge {
//     fn set_status(
//         &mut self,
//         chunk_pos: ChunkPos,
//         lod: LevelOfDetail,
//         status: TimedChunkMeshStatus,
//     ) {
//         self.statuses.lods[lod].insert(chunk_pos, status);
//     }
//
//     /// Get the [`ChunkMeshStatusManager`] associated with this bridge.
//     pub fn chunk_mesh_status_manager(&self) -> &Arc<ChunkMeshStatusManager> {
//         &self.statuses
//     }
//
//     /// Get the status of this chunk at different LODs.
//     pub fn get_statuses(&self, chunk_pos: ChunkPos) -> LodMap<TimedChunkMeshStatus> {
//         self.statuses.get_statuses(chunk_pos)
//     }
//
//     /// "Flush" the queued mesh data. This marks it as ready for extraction so it will be extracted
//     /// next time the extract schedule runs.
//     pub fn flush(&mut self) {
//         self.should_extract = true;
//     }
//
//     /// The number of chunks queued for extraction at this LOD.
//     pub fn queued_additions(&self, lod: LevelOfDetail) -> usize {
//         self.add[lod].len()
//     }
//
//     /// The number of chunks queued for removal from the render world at this LOD.
//     pub fn queued_removals(&self, lod: LevelOfDetail) -> usize {
//         self.remove[lod].len()
//     }
//
//     pub fn is_empty(&self, lod: LevelOfDetail) -> bool {
//         self.queued_additions(lod) == 0 || self.queued_removals(lod) == 0
//     }
//
//     pub fn should_extract(&self) -> bool {
//         self.should_extract
//     }
//
//     /// Try to queue a chunk mesh of a given age and LOD for extraction. Will do nothing if there's
//     /// a newer version either already queued or extracted.
//     pub fn add_chunk_mesh(
//         &mut self,
//         chunk_pos: ChunkPos,
//         lod: LevelOfDetail,
//         tick: u64,
//         mesh_data: ChunkMeshData,
//     ) {
//         let mut has_filled = false;
//
//         // If we already have a newer chunk mesh, then we return early since we should never extract an
//         // older version of a chunk mesh.
//         if let Some(existing_status) = self.statuses.timed_status(lod, chunk_pos) {
//             if existing_status.tick > tick {
//                 return;
//             }
//
//             has_filled = matches!(
//                 existing_status.status,
//                 ChunkMeshStatus::Filled | ChunkMeshStatus::Extracted
//             );
//         }
//
//         let status = mesh_data.status();
//
//         match status {
//             // If the mesh is empty, queue it for removal so that the previous mesh (if it exists) is removed.
//             ChunkMeshStatus::Empty if has_filled => {
//                 self.remove[lod].set(chunk_pos);
//             }
//             // Only queue the mesh for extraction if it's filled.
//             ChunkMeshStatus::Filled => {
//                 self.add[lod].set(chunk_pos, mesh_data);
//             }
//             _ => (),
//         }
//
//         // Even if we don't queue the mesh for extraction we still need to note down its status.
//         self.set_status(chunk_pos, lod, TimedChunkMeshStatus { tick, status });
//     }
//
//     /// Queue a chunk at a given LOD for removal from the render world.
//     pub fn remove_chunk(&mut self, chunk_pos: ChunkPos, lod: LevelOfDetail, tick: u64) {
//         if let Some(existing) = self.statuses.timed_status(lod, chunk_pos) {
//             if existing.tick > tick {
//                 return;
//             }
//         }
//
//         self.statuses.lods[lod].remove(&chunk_pos);
//         self.add[lod].remove(chunk_pos);
//         self.remove[lod].set(chunk_pos);
//     }
//
//     pub fn additions(
//         &self,
//         lod: LevelOfDetail,
//     ) -> impl Iterator<Item = (ChunkPos, &ChunkMeshData)> + '_ {
//         self.add[lod].iter()
//     }
//
//     pub fn removals(&self, lod: LevelOfDetail) -> impl Iterator<Item = ChunkPos> + '_ {
//         self.remove[lod].iter()
//     }
//
//     /// Clear the removal and addition queues and mark the added chunks in the queue as being extracted.
//     /// Also resets the 'Self::should_extract()' status.
//     /// Should be called in the extract stage in the render world after copying data to communicate the
//     /// status of the meshes to the main world.
//     pub fn mark_as_extracted(&mut self, lods: LODs) {
//         self.should_extract = false;
//
//         for lod in lods.contained_lods() {
//             self.remove[lod].clear();
//
//             let additions = &mut self.add[lod];
//             for (chunk_pos, _) in additions.drain() {
//                 self.statuses.lods[lod]
//                     .get_mut(&chunk_pos)
//                     .expect("All chunk positions queued for addition should have a status")
//                     .status = ChunkMeshStatus::Extracted;
//             }
//         }
//     }
// }

pub(crate) static CHUNK_RENDER_TASK_POOL: OnceLock<TaskPool> = OnceLock::new();

/// The name of the threads in the mesh builder task pool.
/// See [`TaskPoolBuilder::thread_name()`] for some more information.
pub static MESH_BUILDER_TASK_POOL_THREAD_NAME: &'static str = "Mesh Builder Task Pool";

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        info!("Initializing mesh controller");

        let mesh_builder_threads = max(1, (available_parallelism() as f32 * 0.75).ceil() as usize);

        CHUNK_RENDER_TASK_POOL.set(
            TaskPoolBuilder::new()
                .num_threads(mesh_builder_threads)
                .thread_name(MESH_BUILDER_TASK_POOL_THREAD_NAME.into())
                .build(),
        ).expect("build() should only be called once, and it's the only place where we initialize the pool");

        app.add_plugins((
            AsyncEventPlugin::<BuildChunkMeshEvent>::default(),
            AsyncEventPlugin::<RemoveChunkMeshEvent>::default(),
            EventFunnelPlugin::<MeshFinishedEvent>::for_new(),
        ))
        .init_resource::<ChunkMeshExtractBridge>()
        .init_resource::<OccluderChunks>();

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
