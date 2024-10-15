use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_bevy_events::{ChannelClosed, EventFunnel};
use bevy::prelude::*;

use bevy::tasks::{Task, TaskPool};
use futures::future::join_all;
use parking_lot::Mutex;
use priority_queue::PriorityQueue;

use crate::topo::world::chunk::ChunkFlags;
use crate::topo::world::{ChunkManager, ChunkPos, ChunkRef};
use crate::util::sync::LockStrategy;

use super::events::{ChunkPopulated, PopulationSource};

/// The task pool that world generation tasks are spawned in. These tasks are very CPU-heavy.
pub(crate) static WORLDGEN_TASK_POOL: OnceLock<TaskPool> = OnceLock::new();

/// The name of the threads in the worldgen task pool.
/// See [`TaskPoolBuilder::thread_name()`] for some more information.
pub static WORLDGEN_TASK_POOL_THREAD_NAME: &'static str = "World Generator Task Pool";

pub const WORLDGEN_WORKER_JOB_QUEUE_LOCK_TIMEOUT: Duration = Duration::from_millis(10);

pub type WorldgenJobQueue = PriorityQueue<ChunkPos, u32, rustc_hash::FxBuildHasher>;

/// Create a new [`WorldgenJobQueue`] with the default hasher.
fn new_worldgen_job_queue() -> WorldgenJobQueue {
    WorldgenJobQueue::with_hasher(rustc_hash::FxBuildHasher::default())
}

pub struct WorldgenWorkerPool {
    pub job_queue: Arc<Mutex<WorldgenJobQueue>>,
    shutdown_interrupt: Arc<AtomicBool>,
    tasks: Vec<Task<()>>,
}

impl WorldgenWorkerPool {
    /// Create a new pool of workers running in the given task pool.
    /// You provide a factory closure to this function which will create the worldgen workers.
    /// Workers can send [`ChunkPopulated`] events through the provided funnel.
    pub fn new<F: Fn() -> Box<dyn WorldgenWorker>>(
        chunk_populated_funnel: EventFunnel<ChunkPopulated>,
        chunk_manager: Arc<ChunkManager>,
        factory: F,
    ) -> Self {
        let Some(task_pool) = WORLDGEN_TASK_POOL.get() else {
            panic!("Worldgen task pool is not initialized");
        };

        let shutdown_interrupt = Arc::new(AtomicBool::new(false));
        let mut tasks = Vec::with_capacity(task_pool.thread_num());
        let job_queue = Arc::new(Mutex::new(new_worldgen_job_queue()));

        info!(
            "World generation worker pool size: {}",
            task_pool.thread_num()
        );

        for _ in 0..task_pool.thread_num() {
            let interrupt = shutdown_interrupt.clone();
            let queue = job_queue.clone();
            let funnel = chunk_populated_funnel.clone();
            let cm = chunk_manager.clone();
            let mut worker = factory();

            let task = task_pool.spawn(async move {
                while !interrupt.load(Ordering::Relaxed) {
                    // Try getting the next job for the given timeout duration. We don't want to hang on the mutex for too long
                    // in case we are ordered to shut down.
                    let Some(mut queue_guard) =
                        queue.try_lock_for(WORLDGEN_WORKER_JOB_QUEUE_LOCK_TIMEOUT)
                    else {
                        continue;
                    };

                    let Some((next_chunk_pos, _)) = queue_guard.pop() else {
                        drop(queue_guard);

                        // This isn't a great way to do this, since this is async code.
                        // But since we're spawning one task per thread, this shouldn't cause any issues.
                        std::thread::sleep(WORLDGEN_WORKER_JOB_QUEUE_LOCK_TIMEOUT);

                        continue;
                    };

                    let cx = WorldgenContext {
                        populated: &funnel,
                        chunk_manager: cm.as_ref(),
                    };

                    WorldgenWorker::run(worker.as_mut(), next_chunk_pos, cx);
                }
            });

            tasks.push(task);
        }

        Self {
            shutdown_interrupt,
            tasks,
            job_queue,
        }
    }

    /// Shut down the workers in this pool and wait for them to finish.
    pub async fn shutdown(&mut self) {
        info!("Shutting down worldgen worker pool");
        self.shutdown_interrupt.store(true, Ordering::Relaxed);
        join_all(self.tasks.drain(..)).await;
    }
}

/// Context for a worldgen job.
pub struct WorldgenContext<'a> {
    populated: &'a EventFunnel<ChunkPopulated>,
    chunk_manager: &'a ChunkManager,
}

impl<'a> WorldgenContext<'a> {
    /// Send a [`ChunkPopulated`] event on the main world to notify the rest of the engine that
    /// a chunk was successfully populated with the world generator and can be read from.
    pub fn notify_done(&self, chunk_pos: ChunkPos) -> Result<(), ChannelClosed<ChunkPos>> {
        self.populated
            .send(ChunkPopulated {
                chunk_pos,
                source: PopulationSource::Worldgen,
            })
            .map_err(|_| ChannelClosed(chunk_pos))
    }

    /// The engine's chunk manager.
    pub fn chunk_manager(&self) -> &ChunkManager {
        &self.chunk_manager
    }

    /// Get the *primordial* chunk at the given position. Will return [`None`] if the
    /// chunk is out of bounds, not loaded, or not primordial.
    pub fn loaded_primordial_chunk(&self, chunk_pos: ChunkPos) -> Option<ChunkRef<'_>> {
        let chunk_ref = self.chunk_manager().loaded_chunk(chunk_pos).ok()?;
        let chunk_flags = chunk_ref.flags(LockStrategy::Blocking).unwrap();

        if chunk_flags.contains(ChunkFlags::PRIMORDIAL) {
            Some(chunk_ref)
        } else {
            None
        }
    }
}

/// An object-safe trait implemented by types that act as worldgen workers.
pub trait WorldgenWorker: Send {
    /// Run a worldgen routine for the given chunk position.
    /// Implementors should notify the engine of their completion through the provided context.
    fn run<'a>(&mut self, chunk_pos: ChunkPos, cx: WorldgenContext<'a>);
}

static_assertions::assert_obj_safe!(WorldgenWorker);
