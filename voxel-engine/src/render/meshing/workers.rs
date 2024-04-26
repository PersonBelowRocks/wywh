use std::{
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use bevy::{
    ecs::system::Resource,
    log::{error, info, warn},
    tasks::{block_on, futures_lite::future, Task, TaskPool},
};
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender};
use dashmap::DashMap;

use crate::{
    data::registries::Registries,
    render::{
        meshing::{error::ChunkMeshingError, Context, MesherOutput},
    },
    topo::{chunk::ChunkPos, realm::ChunkManager},
    util::result::ResultFlattening,
};

use super::Mesher;

pub struct Worker<M: Mesher> {
    task: Task<()>,
    interrupt: Arc<AtomicBool>,
    label: String,
    mesher: PhantomData<M>,
}

#[derive(Clone)]
pub struct WorkerParams<M: Mesher> {
    pub registries: Registries,
    pub chunk_manager: Arc<ChunkManager>,
    pub mesher: M,

    pub finished: FinishedChunks,
    pub cmds: Receiver<MeshCommand>,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, dm::Constructor)]
pub struct MeshCommandId(u32);

#[derive(Clone)]
pub struct MeshCommand {
    pos: ChunkPos,
    id: MeshCommandId,
}

impl<M: Mesher> Worker<M> {
    pub fn new(
        pool: &TaskPool,
        params: WorkerParams<M>,
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

                        let result = cm.with_neighbors::<_, Result<MesherOutput, ChunkMeshingError>>(cmd.pos, |neighbors| {
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
                                params.finished.0.insert(cmd.pos, output);
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
            mesher: PhantomData,
        }
    }

    pub async fn stop(self) {
        self.interrupt.store(true, Ordering::Relaxed);
        future::poll_once(self.task.cancel()).await;
    }
}

#[derive(Clone, Default)]
pub struct FinishedChunks(Arc<DashMap<ChunkPos, MesherOutput>>);

#[derive(Resource)]
pub struct MeshWorkerPool<M: Mesher> {
    workers: Vec<Worker<M>>,
    cmds: Sender<MeshCommand>,
    finished: FinishedChunks,
}

impl<M: Mesher> MeshWorkerPool<M> {
    pub fn new(
        worker_count: usize,
        pool: &TaskPool,
        mesher: M,
        registries: Registries,
        cm: Arc<ChunkManager>,
    ) -> Self {
        let finished = FinishedChunks::default();
        let (cmd_sender, cmd_recver) = channel::unbounded::<MeshCommand>();
        let mut workers = Vec::<Worker<M>>::with_capacity(worker_count);

        let default_channel_timeout_duration = Duration::from_millis(500);

        let worker_params = WorkerParams {
            registries,
            chunk_manager: cm,
            mesher,
            finished: finished.clone(),
            cmds: cmd_recver,
        };

        for i in 0..worker_count {
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
            cmds: cmd_sender,
            finished,
        }
    }

    pub fn shutdown(self) {
        for worker in self.workers.into_iter() {
            block_on(worker.stop());
        }
    }

    pub fn get_new_mesh(&self, pos: ChunkPos) -> Option<MesherOutput> {
        self.finished.0.remove(&pos).map(|t| t.1)
    }

    pub fn queue_job(&self, pos: ChunkPos) {
        self.finished.0.remove(&pos);

        self.cmds
            .send(MeshCommand {
                pos,
                id: MeshCommandId::new(0), // TODO: meshing task ID tracking
            })
            .unwrap();
    }

    pub fn optimize_finished_chunk_buffer(&self) {
        self.finished.0.shrink_to_fit()
    }
}
