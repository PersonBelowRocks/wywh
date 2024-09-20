use std::{
    cmp::max,
    hash::BuildHasher,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

use async_bevy_events::{AsyncEventReader, AsyncEventWriter, EventFunnel};
use bevy::{
    prelude::*,
    tasks::{
        available_parallelism, block_on,
        futures_lite::{future::pending, pin},
        AsyncComputeTaskPool, Task, TaskPool, TaskPoolBuilder,
    },
};
use flume::{Receiver, Sender};
use futures_util::{
    future::{join_all, ready},
    FutureExt, StreamExt,
};
use parking_lot::Mutex;
use priority_queue::PriorityQueue;

use crate::{
    data::registries::Registries,
    render::{
        lod::{LevelOfDetail, LodMap},
        meshing::{
            controller::{events::MeshJobUrgency, ChunkMeshData},
            greedy::algorithm::GreedyMesher,
            Context,
        },
    },
    topo::{
        world::{
            chunk::ChunkFlags, chunk_populator::events::PriorityCalcStrategy, ChunkManager,
            ChunkPos, ChunkRef, VoxelRealm,
        },
        ChunkJobQueue,
    },
    util::{closest_distance, closest_distance_sq, sync::LockStrategy},
};

use super::{
    events::{
        BuildChunkMeshEvent, MeshFinishedEvent, RecalculateMeshBuildingEventPrioritiesEvent,
        RemoveChunkMeshEvent,
    },
    ChunkMeshExtractBridge, ChunkMeshStatusManager,
};

pub(crate) static MESH_BUILDER_TASK_POOL: OnceLock<TaskPool> = OnceLock::new();

/// The name of the threads in the mesh builder task pool.
/// See [`TaskPoolBuilder::thread_name()`] for some more information.
pub static MESH_BUILDER_TASK_POOL_THREAD_NAME: &'static str = "Mesh Builder Task Pool";

pub const MESH_BUILDER_JOB_QUEUE_LOCK_TIMEOUT: Duration = Duration::from_millis(10);

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
    pub staging_chan_senders: LodMap<Sender<MeshBuilderJob>>,
    tasks: Vec<Task<()>>,
}

const DEFAULT_DEBUG_LOD: LevelOfDetail = LevelOfDetail::X16Subdiv;
const MESH_BUILDER_JOB_STAGING_CHANNEL_SIZE: usize = 16;

/// Remove [`ChunkFlags::REMESH`], [`ChunkFlags::REMESH_NEIGHBORS`], and [`ChunkFlags::FRESHLY_GENERATED`] from
/// a chunk's flags.
fn remove_remesh_related_flags(chunk_ref: &ChunkRef) {
    chunk_ref
        .update_flags(LockStrategy::Blocking, |flags| {
            flags.remove(
                ChunkFlags::REMESH | ChunkFlags::REMESH_NEIGHBORS | ChunkFlags::FRESHLY_GENERATED,
            );
        })
        .unwrap();
}

fn staging_channels() -> (
    LodMap<Sender<MeshBuilderJob>>,
    LodMap<Receiver<MeshBuilderJob>>,
) {
    let mut senders = LodMap::new();
    let mut receivers = LodMap::new();

    for lod in LevelOfDetail::LODS {
        let (tx, rx) = flume::bounded(MESH_BUILDER_JOB_STAGING_CHANNEL_SIZE);
        senders.insert(lod, tx);
        receivers.insert(lod, rx);
    }

    (senders, receivers)
}

impl MeshBuilderPool {
    pub fn new(
        registries: Registries,
        chunk_manager: Arc<ChunkManager>,
        finished_meshes: EventFunnel<MeshFinishedEvent>,
    ) -> Self {
        let Some(task_pool) = MESH_BUILDER_TASK_POOL.get() else {
            panic!("Mesh builder task pool is not initialized");
        };

        let mut tasks = Vec::<Task<()>>::with_capacity(task_pool.thread_num());
        let (txs, rxs) = staging_channels();

        info!("Mesh builder pool size: {}", task_pool.thread_num());

        for _ in 0..task_pool.thread_num() {
            // Clone everything so it can be moved into the task.
            let receivers = rxs.clone();
            let cm = chunk_manager.clone();
            let reg = registries.clone();
            let finished = finished_meshes.clone();

            // Add the task to our list of all tasks so we can gracefully shut them down.
            tasks.push(task_pool.spawn(async move {
                // Initialize the mesher and its scratch buffers here
                let mut greedy_mesher = GreedyMesher::new();

                loop {
                    // Try getting the next job for the given timeout duration. We don't want to hang on the mutex for too long
                    // in case we are ordered to shut down.
                    let Ok(job) = receivers[DEFAULT_DEBUG_LOD].recv_async().await else {
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

                    // If the chunk is transparent we don't build the mesh for it since there's no contained data.
                    // Instead we immediatelly send a finished mesh event.
                    if chunk_ref
                        .flags(LockStrategy::Blocking)
                        .unwrap()
                        .contains(ChunkFlags::TRANSPARENT)
                    {
                        let _ = finished.send(MeshFinishedEvent {
                            chunk_pos: job.chunk_pos,
                            lod: DEFAULT_DEBUG_LOD,
                            mesh: ChunkMeshData::empty(DEFAULT_DEBUG_LOD),
                            tick: job.tick,
                        });

                        remove_remesh_related_flags(&chunk_ref);
                        continue;
                    }

                    // Try to grab all the neighbors immediately, if we can't get a neighbor immediately then it's
                    // being written to which means we'll get a separate mesh building event for it once the writing is done.
                    let Ok(mesher_result) =
                        cm.neighbors(job.chunk_pos, LockStrategy::Immediate, |neighbors| {
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
                        Ok(chunk_mesh_data) => {
                            // We don't care about the result here. The most likely reason that we couldn't
                            // send this event is that the app is shutting down, which means we will also shut
                            // down due to the interrupt check.
                            let _ = finished.send(MeshFinishedEvent {
                                chunk_pos: job.chunk_pos,
                                lod: DEFAULT_DEBUG_LOD,
                                mesh: chunk_mesh_data,
                                tick: job.tick,
                            });
                        }
                        Err(error) => error!(
                            "Mesh building job error (CHUNK_POS={} LOD={:?} TICK={}): {error}",
                            job.chunk_pos, DEFAULT_DEBUG_LOD, job.tick
                        ),
                    }

                    remove_remesh_related_flags(&chunk_ref);
                } // Main loop
            }))
        }

        Self {
            staging_chan_senders: txs,
            tasks,
        }
    }

    pub async fn shutdown(&mut self) {
        info!("Shutting down mesh builder pool");
        join_all(self.tasks.drain(..).map(Task::cancel)).await;
    }
}

/// Calculate new priorities for the items in the queue based on the distance returned by the distance function.
/// The distance function's argument is the worldspace center position of a chunk (the chunk that's associated with the [`MeshBuilderJob`]).
/// The priority will be higher the smaller the value returned by the distance function.
/// Formula for the priority is roughly `u32::MAX - distance_fn(chunk_pos)`.
#[inline]
fn calculate_priorities_based_on_distance<F: Fn(Vec3) -> f32>(
    distance_fn: F,
    queue: &mut ChunkJobQueue<MeshBuilderJob>,
) {
    queue.recalculate_priorities(|chunk_pos, _| {
        let center = chunk_pos.worldspace_center();
        let min_distance = distance_fn(center);

        // Closer chunk positions are higher priority, so we need to invert the distance.
        u32::MAX - (min_distance as u32)
    });
}

pub struct MeshBuilderEventProxyTaskState {
    mesh_builder_pool: MeshBuilderPool,
    queues: LodMap<ChunkJobQueue<MeshBuilderJob>>,
    status_manager: Arc<ChunkMeshStatusManager>,
}

impl MeshBuilderEventProxyTaskState {
    pub fn new(
        registries: Registries,
        chunk_manager: Arc<ChunkManager>,
        status_manager: Arc<ChunkMeshStatusManager>,
        finished_meshes: EventFunnel<MeshFinishedEvent>,
    ) -> Self {
        Self {
            mesh_builder_pool: MeshBuilderPool::new(registries, chunk_manager, finished_meshes),
            queues: LodMap::from_fn(|_| Some(ChunkJobQueue::new())),
            status_manager,
        }
    }

    fn queue_if_newer(&mut self, lod: LevelOfDetail, job: MeshBuilderJob, priority: u32) {
        self.queues[lod].push_with(job.chunk_pos, |existing| match existing {
            // Only add if the existing job is older or as old as this job, but use the highest priority of the two.
            Some((ex_priority, ex_job)) if ex_job.tick <= job.tick => {
                Some((max(ex_priority, priority), job))
            }
            // The job does not exist so we can safely add it
            None => Some((priority, job)),
            // Don't add anything if the existing job was younger.
            _ => None,
        });
    }

    pub fn handle_build_mesh_event(&mut self, event: BuildChunkMeshEvent) {
        let MeshJobUrgency::P1(priority) = event.urgency else {
            todo!("Immediate mesh building is not supported yet");
        };

        let job = MeshBuilderJob {
            chunk_pos: event.chunk_pos,
            tick: event.tick,
        };

        // Queue the center chunk
        self.queue_if_newer(event.lod, job, priority);

        // Queue the neighbors of the center chunk
        for selected_neighbor in event.neighbors.selected() {
            let nb_chunk_pos = ChunkPos::from(event.chunk_pos.as_ivec3() + selected_neighbor);
            let job = MeshBuilderJob {
                chunk_pos: nb_chunk_pos,
                tick: event.tick,
            };

            // The reason we even care about the neighboring chunks is in case their old mesh does not
            // line up cleanly with this chunk's mesh. This obviously only happens if the neighboring chunks
            // even have a mesh to begin with, so therefore we exclude all neighbors without a mesh.
            if self.status_manager.contains(event.lod, nb_chunk_pos) {
                self.queue_if_newer(event.lod, job, priority);
            }
        }
    }

    pub fn handle_recalc_priorities_event(
        &mut self,
        event: RecalculateMeshBuildingEventPrioritiesEvent,
    ) {
        for lod in LevelOfDetail::LODS {
            match &event.strategy {
                PriorityCalcStrategy::ClosestDistanceSq(positions) => {
                    calculate_priorities_based_on_distance(
                        |chunk_center| {
                            closest_distance_sq(chunk_center, positions.iter().cloned())
                                .unwrap_or(0.0)
                        },
                        &mut self.queues[lod],
                    );
                }
                PriorityCalcStrategy::ClosestDistance(positions) => {
                    calculate_priorities_based_on_distance(
                        |chunk_center| {
                            closest_distance(chunk_center, positions.iter().cloned()).unwrap_or(0.0)
                        },
                        &mut self.queues[lod],
                    );
                }
            }
        }
    }

    pub fn handle_remove_mesh_event(&mut self, event: RemoveChunkMeshEvent) {
        for lod in event.lods.contained_lods() {
            let Some(job) = self.queues[lod].get(event.chunk_pos) else {
                continue;
            };

            // The existing job is newer than the removal event
            if job.tick >= event.tick {
                continue;
            }

            self.queues[lod].remove(event.chunk_pos);
        }
    }

    pub async fn on_shutdown(&mut self) {
        self.mesh_builder_pool.shutdown().await;
    }
}

#[derive(Resource)]
pub struct MeshBuilderEventProxyTaskHandle {
    shutdown_tx: Sender<()>,
    task: Option<Task<()>>,
}

impl Drop for MeshBuilderEventProxyTaskHandle {
    fn drop(&mut self) {
        if self.shutdown_tx.send(()).is_err() {
            warn!("Shutdown channel for mesh builder event proxy was disconnected");
        }

        block_on(self.task.take().unwrap())
    }
}

/// This system starts the mesh builder pool and the proxy task that forwards events to the pool.
pub fn start_mesh_builder_tasks(
    mut cmds: Commands,
    realm: VoxelRealm,
    registries: Res<Registries>,
    extract_bridge: Res<ChunkMeshExtractBridge>,
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
        registries.clone(),
        realm.clone_cm(),
        extract_bridge.chunk_mesh_status_manager().clone(),
        mesh_finished_funnel.clone(),
    );

    // Shutdown one-shot channel
    let (shutdown_tx, shutdown_rx) = flume::bounded::<()>(1);

    let task = AsyncComputeTaskPool::get().spawn(async move {
        let mut build_mesh_events_stream = build_mesh_events.stream();
        let mut recalc_priority_events_stream = recalc_priority_events.stream();
        let mut remove_chunk_mesh_events_stream = remove_chunk_mesh_events.stream();
        let mut shutdown_stream = shutdown_rx.stream();
        let job_txs = task_state.mesh_builder_pool.staging_chan_senders.clone();

        let mut next_job = None::<MeshBuilderJob>;

        'task_loop: loop {
            if next_job.is_none() {
                next_job = task_state.queues[DEFAULT_DEBUG_LOD]
                    .pop()
                    .map(|(_, job)| job);
            }

            let nv = next_job.clone();
            let send_task = async {
                match nv {
                    Some(job) => job_txs[DEFAULT_DEBUG_LOD].send_async(job).await,
                    None => pending().await,
                }
            }
            .fuse();

            pin!(send_task);

            futures_util::select! {
                _ = shutdown_stream.next() => {
                    break 'task_loop;
                },

                _ = send_task => {
                    next_job = None;
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
                },
            };
        }

        task_state.on_shutdown().await;
    });

    cmds.insert_resource(MeshBuilderEventProxyTaskHandle {
        shutdown_tx,
        task: Some(task),
    });
}
