use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use bevy::{
    ecs::system::Resource,
    log::{error, warn},
    tasks::{block_on, futures_lite::future, Task, TaskPool},
};
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender, TrySendError};

use crate::{
    data::registries::Registries,
    render::{
        lod::LevelOfDetail,
        meshing::{error::ChunkMeshingError, greedy::algorithm::GreedyMesher, Context},
    },
    topo::world::{chunk::LockStrategy, ChunkManager, ChunkPos},
    util::{result::ResultFlattening, ChunkIndexMap, ChunkSet, Keyed},
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

    pub finished: Sender<FinishedChunkMeshData>,
    pub cmds: Receiver<MeshBuilderCommand>,
}

#[derive(Clone)]
pub struct MeshBuilderCommand {
    pub pos: ChunkPos,
    pub lod: LevelOfDetail,
    pub priority: RemeshPriority,
    pub generation: u64,
}

impl Keyed<RemeshPriority> for MeshBuilderCommand {
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
            let mut backlog_cmd = None::<MeshBuilderCommand>;

            while !task_interrupt.load(Ordering::Relaxed) {
                let cmd = match backlog_cmd.take() {
                    Some(cmd) => Some(cmd),
                    None => match params.cmds.recv_timeout(channel_timeout) {
                        Ok(cmd) => Some(cmd),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => {
                            warn!("Channel disconnected for meshing worker '{task_label}', worker is shutting down.");
                            return;
                        }
                    }
                };

                let Some(cmd) = cmd else { continue };

                let cm = params.chunk_manager.clone();

                let result = cm.with_neighbors(LockStrategy::Blocking, cmd.pos, |neighbors| {
                    let context = Context {
                        lod: cmd.lod,
                        neighbors,
                        registries: &params.registries,
                    };

                    let chunk = cm.get_loaded_chunk(cmd.pos, false)?;
                    let handle = chunk.chunk().read_handle(LockStrategy::Blocking).unwrap();
                    Ok(params.mesher.build(handle, context)?)
                }).map_err(ChunkMeshingError::from).custom_flatten();

                match result {
                    Ok(output) => {
                        params.finished.send(FinishedChunkMeshData {
                            data: output,
                            lod: cmd.lod,
                            pos: cmd.pos,
                            tick: cmd.generation
                        }).unwrap();
                    }
                    Err(ChunkMeshingError::ChunkManagerError(error)) => {
                        // backlog if globally locked
                        if error.is_globally_locked() {
                            backlog_cmd = Some(cmd);
                            // sleep here to avoid busy looping
                            thread::sleep(channel_timeout);
                        }

                        continue;
                    },
                    Err(ChunkMeshingError::MesherError(error)) => {
                        error!("Error in worker '{task_label}' building chunk mesh for {}: {error}", cmd.pos);
                    }
                }
            }
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

pub struct FinishedChunkMeshData {
    pub pos: ChunkPos,
    pub lod: LevelOfDetail,
    pub data: ChunkMeshData,
    pub tick: u64,
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
    cmds: Sender<MeshBuilderCommand>,
    pending: ChunkIndexMap<MeshBuilderCommand>,
    finished: Receiver<FinishedChunkMeshData>,
}

impl MeshBuilder {
    pub fn new(
        settings: MeshBuilderSettings,
        pool: &TaskPool,
        registries: Registries,
        cm: Arc<ChunkManager>,
    ) -> Self {
        let (cmd_sender, cmd_recver) =
            channel::bounded::<MeshBuilderCommand>(settings.job_channel_capacity);
        let (mesh_sender, mesh_recver) = channel::unbounded::<FinishedChunkMeshData>();
        let mut workers = Vec::<Worker>::with_capacity(settings.workers);

        let default_channel_timeout_duration = Duration::from_millis(50);

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
            pending: ChunkIndexMap::default(),
            cmds: cmd_sender,
            finished: mesh_recver,
        }
    }

    /// Queue jobs for workers based on their priority. The provided commands may not be sent immediately.
    /// Internally this function places the commands in a sorted buffer ordered by the command priority.
    /// Then it pops the highest-priority commands out of the buffer until the internal channel used to communicate
    /// with workers is full. This function must be called periodically (even with just an empty iterator)
    /// to send the next commands to the workers.
    pub fn queue_jobs<I: Iterator<Item = MeshBuilderCommand>>(&mut self, cmds: I) {
        self.pending.extend(cmds.map(|c| (c.pos, c)));
        self.pending
            .sort_unstable_by(|_, l, _, r| r.priority.cmp(&l.priority));

        #[cfg(debug_assertions)]
        if !self.pending.is_empty() {
            debug_assert!(
                self.pending.first().unwrap().1.priority <= self.pending.last().unwrap().1.priority
            );
        }

        while let Some((_, next)) = self.pending.pop() {
            if let Err(error) = self.cmds.try_send(next) {
                match error {
                    TrySendError::Disconnected(msg) => {
                        self.pending.insert(msg.pos, msg);
                        error!("Could not send remesh command to workers because the channel is disconnected.");
                        break;
                    }
                    TrySendError::Full(msg) => {
                        self.pending.insert(msg.pos, msg);
                        break;
                    }
                }
            }
        }
    }

    /// Removes the given chunks from the pending commands.
    pub fn remove_pending(&mut self, remove: &ChunkSet) {
        self.pending.retain(|chunk, _| !remove.contains(*chunk));
    }

    pub fn shutdown(self) {
        for worker in self.workers.into_iter() {
            block_on(worker.stop());
        }
    }

    pub fn get_finished_meshes(&self) -> Vec<FinishedChunkMeshData> {
        let mut vec = Vec::with_capacity(self.finished.len());

        while let Ok(finished) = self.finished.try_recv() {
            vec.push(finished);
        }

        vec
    }
}
