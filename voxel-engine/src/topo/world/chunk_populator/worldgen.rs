use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bevy::prelude::*;

use bevy::tasks::{available_parallelism, Task, TaskPool, TaskPoolBuilder};
use futures_util::future::join_all;
use parking_lot::Mutex;
use priority_queue::PriorityQueue;

use crate::topo::world::ChunkPos;

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
    pub fn new<F: Fn() -> Box<dyn WorldgenWorker>>(task_pool: &TaskPool, factory: F) -> Self {
        let shutdown_interrupt = Arc::new(AtomicBool::new(false));
        let mut tasks = Vec::with_capacity(task_pool.thread_num());
        let job_queue = Arc::new(Mutex::new(new_worldgen_job_queue()));

        for _ in 0..task_pool.thread_num() {
            let interrupt = shutdown_interrupt.clone();
            let queue = job_queue.clone();
            let mut worker = factory();

            let task = task_pool.spawn(async move {
                while !interrupt.load(Ordering::Relaxed) {
                    // Try getting the next job for the given timeout duration. We don't want to hang on the mutex for too long
                    // in case we are ordered to shut down.
                    let Some(Some((next_chunk_pos, _))) = queue
                        .try_lock_for(WORLDGEN_WORKER_JOB_QUEUE_LOCK_TIMEOUT)
                        .map(|mut guard| guard.pop())
                    else {
                        continue;
                    };

                    WorldgenWorker::run(worker.as_mut(), next_chunk_pos)
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
        self.shutdown_interrupt.store(true, Ordering::Relaxed);
        join_all(self.tasks.drain(..)).await;
    }
}

pub trait WorldgenWorker: Send {
    fn run(&mut self, chunk_pos: ChunkPos);
}

static_assertions::assert_obj_safe!(WorldgenWorker);
