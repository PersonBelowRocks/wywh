pub mod events;
mod state_tracking;

use async_bevy_events::{AsyncEventPlugin, EventFunnelPlugin};
use bevy::tasks::TaskPool;
use bevy::{
    prelude::*,
    tasks::{TaskPoolBuilder, available_parallelism},
};
use events::{BuildChunkMeshEvent, MeshFinishedEvent, RemoveChunkMeshEvent};
use std::sync::OnceLock;
use std::{
    cmp::{self, max},
    sync::Arc,
};

use crate::render::meshing::controller::state_tracking::{ChunkMeshState, ChunkMeshTimestate};
use crate::{
    CoreEngineSetup, EngineState,
    render::{
        lod::{LODs, LevelOfDetail, LodMap},
        quad::GpuQuad,
    },
    topo::world::ChunkPos,
    util::{ChunkMap, ChunkSet},
};

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
        ));

        // app.add_systems(
        //     PreUpdate,
        //     (
        //         // TODO: send mesh building events when necessary!
        //     )
        //         .chain()
        //         .run_if(in_state(EngineState::Finished)),
        // );
    }
}
