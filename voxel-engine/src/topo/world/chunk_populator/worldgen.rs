use std::future::Future;
use std::sync::Arc;

use async_lock::Mutex;
use bevy::prelude::*;

use bevy::tasks::{available_parallelism, TaskPool, TaskPoolBuilder};
use indexmap::{IndexMap, IndexSet};
use priority_queue::PriorityQueue;
use xtra::Address;

use crate::topo::world::chunk_manager::ecs::ChunkManagerRes;
use crate::topo::world::{ChunkManager, ChunkPos, VoxelRealm};

/// The name of the threads in the worldgen task pool.
/// See [`TaskPoolBuilder::thread_name()`] for some more information.
pub static WORLDGEN_TASK_POOL_THREAD_NAME: &'static str = "World Generator Task Pool";

/// The task pool that world generation tasks are spawned in. These tasks are very CPU-heavy.
#[derive(Resource, Deref)]
pub struct WorldgenTaskPool(Arc<TaskPool>);

impl WorldgenTaskPool {
    /// Creates a worldgen task pool with the given number of tasks.
    pub fn new(tasks: usize) -> Self {
        let task_pool = TaskPoolBuilder::new()
            .num_threads(tasks)
            .thread_name(WORLDGEN_TASK_POOL_THREAD_NAME.into())
            .build();
        Self(Arc::new(task_pool))
    }
}

impl Default for WorldgenTaskPool {
    fn default() -> Self {
        let cores = available_parallelism();
        Self::new(cores)
    }
}

// TODO: docs
#[derive(Copy, Clone, Debug)]
pub struct GenerateChunk(pub ChunkPos);

// TODO: docs
#[derive(xtra::Actor)]
pub struct GenerateChunkActor {
    pub id: usize,
    pub chunk_manager: Arc<ChunkManager>,
}

impl xtra::Handler<GenerateChunk> for GenerateChunkActor {
    type Return = ();

    fn handle(
        &mut self,
        message: GenerateChunk,
        ctx: &mut xtra::Context<Self>,
    ) -> impl Future<Output = Self::Return> + Send {
        async { todo!() }
    }
}
