use std::{
    cmp::max,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_bevy_events::{AsyncEventReader, AsyncEventWriter, EventFunnel};
use bevy::{
    prelude::*,
    tasks::{available_parallelism, AsyncComputeTaskPool, Task, TaskPool, TaskPoolBuilder},
};
use flume::Sender;
use futures_util::{future::join_all, StreamExt};
use parking_lot::Mutex;
use priority_queue::PriorityQueue;

use crate::{
    data::registries::Registries,
    render::{
        lod::{LevelOfDetail, LodMap},
        meshing::{controller::events::MeshJobUrgency, greedy::algorithm::GreedyMesher, Context},
    },
    topo::world::{chunk::ChunkFlags, ChunkManager, ChunkPos, VoxelRealm},
    util::sync::LockStrategy,
};

use super::events::{
    BuildChunkMeshEvent, MeshFinishedEvent, RecalculateMeshBuildingEventPrioritiesEvent,
    RemoveChunkMeshEvent,
};

/// The name of the threads in the mesh builder task pool.
/// See [`TaskPoolBuilder::thread_name()`] for some more information.
pub static MESH_BUILDER_TASK_POOL_THREAD_NAME: &'static str = "Mesh Builder Task Pool";

pub const MESH_BUILDER_JOB_QUEUE_LOCK_TIMEOUT: Duration = Duration::from_millis(10);

#[derive(Resource, Deref)]
pub struct MeshBuilderTaskPool(Arc<TaskPool>);

impl MeshBuilderTaskPool {
    /// Creates a mesh builder task pool with the given number of tasks.
    pub fn new(tasks: usize) -> Self {
        let task_pool = TaskPoolBuilder::new()
            .num_threads(tasks)
            .thread_name(MESH_BUILDER_TASK_POOL_THREAD_NAME.into())
            .build();
        Self(Arc::new(task_pool))
    }
}

impl Default for MeshBuilderTaskPool {
    fn default() -> Self {
        let cores = available_parallelism();
        Self::new(cores / 2)
    }
}

/// An item in the mesh building job queue. Ignores its `tick` field in equality checks and hashing.
#[derive(Copy, Clone)]
pub struct MeshBuilderJob {
    pub chunk_pos: ChunkPos,
    pub tick: u64,
}

impl std::hash::Hash for MeshBuilderJob {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.chunk_pos.hash(state)
    }
}

impl std::cmp::PartialEq for MeshBuilderJob {
    fn eq(&self, other: &Self) -> bool {
        self.chunk_pos == other.chunk_pos
    }
}

impl std::cmp::Eq for MeshBuilderJob {}

/// A queue of mesh building jobs. Automatically handles logic around job recency and priority.
pub struct TickedMeshJobQueue(PriorityQueue<MeshBuilderJob, u32, rustc_hash::FxBuildHasher>);

impl TickedMeshJobQueue {
    pub fn new() -> Self {
        Self(PriorityQueue::with_hasher(
            rustc_hash::FxBuildHasher::default(),
        ))
    }

    pub fn push(&mut self, chunk_pos: ChunkPos, tick: u64, priority: u32) {
        let job = MeshBuilderJob { chunk_pos, tick };

        // If a job already existed for this chunk position, update it so that its as recent as this
        // job and bump its priority if needed.
        if let Some((existing, &priority)) = self.0.get_mut(&job) {
            // Mutation here is okay since the tick field does not affect the hash result or equality.
            existing.tick = max(tick, existing.tick);

            // Set the priority to whichever was highest
            self.0.change_priority_by(&job, |existing_priority| {
                *existing_priority = max(priority, *existing_priority);
            });
        } else {
            self.0.push(job, priority);
        }
    }

    pub fn pop(&mut self) -> Option<MeshBuilderJob> {
        self.0.pop().map(|pair| pair.0)
    }

    pub fn remove(&mut self, chunk_pos: ChunkPos, tick: u64) {
        // The tick field is "hidden" for hashing and equality operations, so this is basically a chunk position key
        let job = MeshBuilderJob {
            chunk_pos,
            // Doesn't matter, we just need the chunk position
            tick: 0,
        };

        let Some((contained, _)) = self.0.get(&job) else {
            return;
        };

        // Only remove the job if it's older than the removal operation
        if contained.tick < tick {
            self.0.remove(&job);
        }
    }
}

pub struct MeshBuilderPool {
    pub job_queues: Arc<LodMap<Mutex<TickedMeshJobQueue>>>,
    shutdown_interrupt: Arc<AtomicBool>,
    tasks: Vec<Task<()>>,
}

fn new_lod_job_queues() -> Arc<LodMap<Mutex<TickedMeshJobQueue>>> {
    Arc::new(LodMap::from_fn(|_| {
        Some(Mutex::new(TickedMeshJobQueue::new()))
    }))
}

const DEFAULT_DEBUG_LOD: LevelOfDetail = LevelOfDetail::X16Subdiv;

impl MeshBuilderPool {
    pub fn new(
        task_pool: &TaskPool,
        finished_meshes: EventFunnel<MeshFinishedEvent>,
        chunk_manager: Arc<ChunkManager>,
        registries: Registries,
    ) -> Self {
        let shutdown_interrupt = Arc::new(AtomicBool::new(false));
        let mut tasks = Vec::<Task<()>>::with_capacity(task_pool.thread_num());
        let job_queues = new_lod_job_queues();

        info!("Mesh builder pool size: {}", task_pool.thread_num());

        for _ in 0..task_pool.thread_num() {
            // Clone everything so it can be moved into the task.
            let interrupt = shutdown_interrupt.clone();
            let queues = job_queues.clone();
            let cm = chunk_manager.clone();
            let reg = registries.clone();
            let finished = finished_meshes.clone();

            // Add the task to our list of all tasks so we can gracefully shut them down.
            tasks.push(task_pool.spawn(async move {
                // Initialize the mesher and its scratch buffers here
                let mut greedy_mesher = GreedyMesher::new();

                while !interrupt.load(Ordering::Relaxed) {
                    // Try getting the next job for the given timeout duration. We don't want to hang on the mutex for too long
                    // in case we are ordered to shut down.
                    let Some(mut queue_guard) =
                        queues[DEFAULT_DEBUG_LOD].try_lock_for(MESH_BUILDER_JOB_QUEUE_LOCK_TIMEOUT)
                    else {
                        continue;
                    };

                    let Some(job) = queue_guard.pop() else {
                        drop(queue_guard);

                        // This isn't a great way to do this, since this is async code.
                        // But since we're spawning one task per thread, this shouldn't cause any issues.
                        std::thread::sleep(MESH_BUILDER_JOB_QUEUE_LOCK_TIMEOUT);

                        continue;
                    };

                    let Ok(chunk_ref) = cm.loaded_chunk(job.chunk_pos) else {
                        continue;
                    };

                    // We don't want to try reading data from primordial chunks. If a mesh building event
                    // was sent for a primordial chunk it's either a mistake or the chunk was reloaded before
                    // the event could be processed. Either way the most sane and safe thing to do is ignore it.
                    if chunk_ref.is_primordial() {
                        continue;
                    }

                    let Ok(mesher_result) =
                        cm.neighbors(job.chunk_pos, LockStrategy::Blocking, |neighbors| {
                            let read_handle = chunk_ref
                                .chunk()
                                .read_handle(LockStrategy::Blocking)
                                .unwrap();

                            let context = Context {
                                lod: DEFAULT_DEBUG_LOD,
                                neighbors,
                                registries: &reg,
                            };

                            greedy_mesher.build(read_handle, context)
                        })
                    else {
                        continue;
                    };

                    match mesher_result {
                        Ok(chunk_mesh_data) => finished
                            .send(MeshFinishedEvent {
                                chunk_pos: job.chunk_pos,
                                lod: DEFAULT_DEBUG_LOD,
                                mesh: chunk_mesh_data,
                                tick: job.tick,
                            })
                            .unwrap(),
                        Err(error) => error!(
                            "Mesh building job error (CHUNK_POS={} LOD={:?} TICK={}): {error}",
                            job.chunk_pos, DEFAULT_DEBUG_LOD, job.tick
                        ),
                    }

                    chunk_ref
                        .update_flags(LockStrategy::Blocking, |flags| {
                            flags.remove(
                                ChunkFlags::REMESH
                                    | ChunkFlags::REMESH_NEIGHBORS
                                    | ChunkFlags::FRESHLY_GENERATED,
                            );
                        })
                        .unwrap();
                }
            }))
        }

        Self {
            job_queues,
            shutdown_interrupt,
            tasks,
        }
    }

    pub async fn shutdown(&mut self) {
        self.shutdown_interrupt.store(true, Ordering::Relaxed);
        join_all(self.tasks.drain(..)).await;
    }
}

pub struct MeshBuilderEventProxyTaskState {
    mesh_builder_pool: MeshBuilderPool,
}

impl MeshBuilderEventProxyTaskState {
    pub fn new(
        mesh_builder_task_pool: &TaskPool,
        chunk_manager: Arc<ChunkManager>,
        registries: Registries,
        finished_meshes: EventFunnel<MeshFinishedEvent>,
    ) -> Self {
        Self {
            mesh_builder_pool: MeshBuilderPool::new(
                mesh_builder_task_pool,
                finished_meshes,
                chunk_manager,
                registries,
            ),
        }
    }

    pub fn handle_build_mesh_event(&mut self, event: BuildChunkMeshEvent) {
        let MeshJobUrgency::P1(priority) = event.urgency else {
            todo!("Immediate mesh building is not supported yet");
        };

        self.mesh_builder_pool.job_queues[event.lod].lock().push(
            event.chunk_pos,
            event.tick,
            priority,
        );
    }

    pub fn handle_recalc_priorities_event(
        &mut self,
        event: RecalculateMeshBuildingEventPrioritiesEvent,
    ) {
        for lod in LevelOfDetail::LODS {
            let mut guard = self.mesh_builder_pool.job_queues[lod].lock();

            todo!();
        }
    }

    pub fn handle_remove_mesh_event(&mut self, event: RemoveChunkMeshEvent) {
        for lod in event.lods.contained_lods() {
            let mut guard = self.mesh_builder_pool.job_queues[lod].lock();
            guard.remove(event.chunk_pos, event.tick);
        }
    }

    pub async fn on_shutdown(&mut self) {
        self.mesh_builder_pool.shutdown().await;
    }
}

#[derive(Resource)]
pub struct MeshBuilderEventProxyTaskHandle {
    shutdown_tx: Sender<()>,
    task: Task<()>,
}

/// This system starts the mesh builder pool and the proxy task that forwards events to the pool.
pub fn start_mesh_builder_tasks(
    mut cmds: Commands,
    mesh_builder_task_pool: Res<MeshBuilderTaskPool>,
    realm: VoxelRealm,
    registries: Res<Registries>,
    build_mesh_events: Res<AsyncEventReader<BuildChunkMeshEvent>>,
    recalc_priority_events: Res<AsyncEventReader<RecalculateMeshBuildingEventPrioritiesEvent>>,
    remove_chunk_mesh_events: Res<AsyncEventReader<RemoveChunkMeshEvent>>,
    mesh_finished_funnel: Res<EventFunnel<MeshFinishedEvent>>,
) {
    info!("Starting background mesh builder pool and mesh event proxy task.");

    let build_mesh_events = build_mesh_events.clone();
    let recalc_priority_events = recalc_priority_events.clone();
    let remove_chunk_mesh_events = remove_chunk_mesh_events.clone();

    let mut task_state = MeshBuilderEventProxyTaskState::new(
        &mesh_builder_task_pool,
        realm.clone_cm(),
        registries.clone(),
        mesh_finished_funnel.clone(),
    );

    // Shutdown one-shot channel
    let (shutdown_tx, shutdown_rx) = flume::bounded::<()>(1);

    let task = AsyncComputeTaskPool::get().spawn(async move {
        let mut build_mesh_events_stream = build_mesh_events.stream();
        let mut recalc_priority_events_stream = recalc_priority_events.stream();
        let mut remove_chunk_mesh_events_stream = remove_chunk_mesh_events.stream();
        let mut shutdown_stream = shutdown_rx.stream();

        'task_loop: loop {
            futures_util::select! {
                _ = shutdown_stream.next() => {
                    break 'task_loop;
                },
                event = build_mesh_events_stream.next() => {
                    let Some(event) = event else {
                        continue;
                    };

                    task_state.handle_build_mesh_event(event);
                },
                event = recalc_priority_events_stream.next() => {
                    let Some(event) = event else {
                        continue;
                    };

                    task_state.handle_recalc_priorities_event(event);
                },
                event = remove_chunk_mesh_events_stream.next() => {
                    let Some(event) = event else {
                        continue;
                    };

                    task_state.handle_remove_mesh_event(event);
                }
            };
        }

        task_state.on_shutdown().await;
    });

    cmds.insert_resource(MeshBuilderEventProxyTaskHandle { shutdown_tx, task });
}
