use std::{
    collections::BinaryHeap,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use bevy::{
    ecs::system::Resource,
    log::{error, info, warn},
    tasks::{block_on, futures_lite::future, Task, TaskPool},
};
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender, TrySendError};
use dashmap::DashMap;

use crate::{
    data::registries::Registries,
    render::meshing::{error::ChunkMeshingError, greedy::algorithm::GreedyMesher, Context},
    topo::world::{ChunkManager, ChunkPos},
    util::{result::ResultFlattening, Keyed, KeyedOrd, SyncChunkMap},
};

use super::{ChunkMeshData, RemeshPriority};

pub struct Worker {
    task: Task<()>,
    interrupt: Arc<AtomicBool>,
    label: String,
}

#[derive(Clone)]
pub struct WorkerParams {
    pub registries: Registries,
    pub chunk_manager: Arc<ChunkManager>,
    pub mesher: GreedyMesher,

    pub finished: Sender<FinishedChunkData>,
    pub cmds: Receiver<MeshCommand>,
}

#[derive(Clone)]
pub struct MeshCommand {
    pub pos: ChunkPos,
    pub priority: RemeshPriority,
    pub generation: u64,
}

impl Keyed<RemeshPriority> for MeshCommand {
    type Key = RemeshPriority;

    fn key(&self) -> &Self::Key {
        &self.priority
    }
}

impl Worker {
    pub fn new(
        pool: &TaskPool,
        mut params: WorkerParams,
        channel_timeout: Duration,
        label: String,
    ) -> Self {
        let atomic_interrupt = Arc::new(AtomicBool::new(false));

        let task_label = label.clone();
        let task_interrupt = atomic_interrupt.clone();
        let task = pool.spawn(async move {
            while !task_interrupt.load(Ordering::Relaxed) {
                match params.cmds.recv_timeout(channel_timeout) {
                    Ok(cmd) => {
                        let cm = params.chunk_manager.clone();

                        let result = cm.with_neighbors::<_, Result<ChunkMeshData, ChunkMeshingError>>(cmd.pos, |neighbors| {
                            let context = Context {
                                neighbors,
                                registries: &params.registries,
                            };

                            let chunk = cm.get_loaded_chunk(cmd.pos)?;
                            Ok(chunk.with_read_access(|access| {
                                params.mesher.build(access, context)
                            })??)
                        }).map_err(ChunkMeshingError::from).custom_flatten();

                        match result {
                            Ok(output) => {
                                params.finished.send(FinishedChunkData {
                                    data: output,
                                    pos: cmd.pos,
                                    generation: cmd.generation
                                }).unwrap();
                            }
                            Err(err) => error!("Error in worker '{task_label}' building chunk mesh: {err}"),
                        }
                    },
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        warn!("Channel disconnected for meshing worker '{task_label}', worker is shutting down.");
                        return;
                    }
                }
            }

            info!("Meshing worker '{task_label}' was interrupted and is shutting down.")
        });

        Self {
            interrupt: atomic_interrupt,
            task,
            label: label.clone(),
        }
    }

    pub async fn stop(self) {
        self.interrupt.store(true, Ordering::Relaxed);
        future::poll_once(self.task.cancel()).await;
    }
}

pub struct FinishedChunkData {
    pub pos: ChunkPos,
    pub data: ChunkMeshData,
    pub generation: u64,
}

#[derive(Copy, Clone)]
pub struct MeshBuilderSettings {
    pub workers: usize,
    pub job_channel_capacity: usize,
    // TODO: if a worker cant send its finished mesh immediately, then let it build another while waiting
    pub worker_mesh_backlog_capacity: usize,
}

#[derive(Resource)]
pub struct MeshBuilder {
    workers: Vec<Worker>,
    cmds: Sender<MeshCommand>,
    pending: BinaryHeap<KeyedOrd<MeshCommand, RemeshPriority>>,
    finished: Receiver<FinishedChunkData>,
}

impl MeshBuilder {
    pub fn new(
        settings: MeshBuilderSettings,
        pool: &TaskPool,
        registries: Registries,
        cm: Arc<ChunkManager>,
    ) -> Self {
        let (cmd_sender, cmd_recver) =
            channel::bounded::<MeshCommand>(settings.job_channel_capacity);
        let (mesh_sender, mesh_recver) = channel::unbounded::<FinishedChunkData>();
        let mut workers = Vec::<Worker>::with_capacity(settings.workers);

        let default_channel_timeout_duration = Duration::from_millis(500);

        let worker_params = WorkerParams {
            registries,
            chunk_manager: cm,
            mesher: GreedyMesher::new(),
            finished: mesh_sender,
            cmds: cmd_recver,
        };

        for i in 0..settings.workers {
            let worker = Worker::new(
                pool,
                worker_params.clone(),
                default_channel_timeout_duration,
                format!("mesh_worker_{i}"),
            );

            workers.push(worker);
        }

        Self {
            workers,
            pending: BinaryHeap::default(),
            cmds: cmd_sender,
            finished: mesh_recver,
        }
    }

    pub fn queue_jobs<I: Iterator<Item = MeshCommand>>(&mut self, cmds: I) {
        self.pending.extend(cmds.map(KeyedOrd::new));

        while let Some(next) = self.pending.pop() {
            let next = next.into_inner();

            if let Err(error) = self.cmds.try_send(next) {
                match error {
                    TrySendError::Disconnected(msg) => {
                        self.pending.push(KeyedOrd::new(msg));
                        error!("Could not send remesh command to workers because the channel is disconnected.");
                        break;
                    }
                    TrySendError::Full(msg) => {
                        self.pending.push(KeyedOrd::new(msg));
                        break;
                    }
                }
            }
        }
    }

    pub fn shutdown(self) {
        for worker in self.workers.into_iter() {
            block_on(worker.stop());
        }
    }

    pub fn get_finished_meshes(&self) -> Vec<FinishedChunkData> {
        let mut vec = Vec::with_capacity(self.finished.len());

        while let Ok(finished) = self.finished.try_recv() {
            vec.push(finished);
        }

        vec
    }
}
