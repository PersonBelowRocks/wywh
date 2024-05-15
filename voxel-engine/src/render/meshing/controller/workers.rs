use std::{
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
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender};
use dashmap::DashMap;

use crate::{
    data::registries::Registries,
    render::meshing::{
        error::ChunkMeshingError, greedy::algorithm::GreedyMesher, Context, Mesher, MesherOutput,
    },
    topo::world::{ChunkManager, ChunkPos},
    util::{result::ResultFlattening, SyncChunkMap},
};

use super::ChunkMeshData;

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
    pos: ChunkPos,
    generation: u64,
}

impl Worker {
    pub fn new(
        pool: &TaskPool,
        params: WorkerParams,
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
                                // Only send the mesh data if the mesher actually produced something
                                if !output.indices.is_empty() && !output.quads.is_empty() {
                                    params.finished.send(FinishedChunkData {
                                        data: ChunkMeshData {
                                            index_buffer: output.indices,
                                            quads: output.quads,
                                        },
                                        pos: cmd.pos,
                                        generation: cmd.generation
                                    }).unwrap();
                                }
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

#[derive(Resource)]
pub struct MeshWorkerPool {
    workers: Vec<Worker>,
    cmds: Sender<MeshCommand>,
    finished: Receiver<FinishedChunkData>,
}

impl MeshWorkerPool {
    pub fn new(
        worker_count: usize,
        pool: &TaskPool,
        mesher: GreedyMesher,
        registries: Registries,
        cm: Arc<ChunkManager>,
    ) -> Self {
        let (cmd_sender, cmd_recver) = channel::unbounded::<MeshCommand>();
        let (mesh_sender, mesh_recver) = channel::unbounded::<FinishedChunkData>();
        let mut workers = Vec::<Worker>::with_capacity(worker_count);

        let default_channel_timeout_duration = Duration::from_millis(500);

        let worker_params = WorkerParams {
            registries,
            chunk_manager: cm,
            mesher,
            finished: mesh_sender,
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
            finished: mesh_recver,
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

    pub fn queue_job(&self, pos: ChunkPos, generation: u64) {
        self.cmds.send(MeshCommand { pos, generation }).unwrap();
    }
}
