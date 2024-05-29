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
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender};

use crate::data::registries::Registries;

use self::generator::Generator;

use super::world::{chunk::ChunkFlags, chunk_manager::GlobalLockState, ChunkManager, ChunkPos};

pub mod ecs;
pub mod error;
pub mod generator;

pub struct Worker {
    task: Task<()>,
    interrupt: Arc<AtomicBool>,
    label: String,
}

#[derive(Clone)]
pub struct WorkerParams {
    pub registries: Registries,
    pub chunk_manager: Arc<ChunkManager>,
    pub cmds: Receiver<GeneratorCommand>,
    pub timeout: Duration,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, dm::Constructor)]
pub struct GeneratorCommandId(u32);

#[derive(Clone)]
pub struct GeneratorCommand {
    pos: ChunkPos,
    id: GeneratorCommandId,
}

async fn internal_worker_task(
    generator: Generator,
    params: WorkerParams,
    interrupt: Arc<AtomicBool>,
    label: String,
) {
    let cm = params.chunk_manager;
    let mut backlog_cmd = None::<GeneratorCommand>;

    while !interrupt.load(Ordering::Relaxed) {
        // Try to get the command from the backlog before we get a new one.
        let cmd = match backlog_cmd.take() {
            Some(cmd) => Some(cmd),
            None => match params.cmds.recv_timeout(params.timeout) {
                Ok(cmd) => Some(cmd),
                Err(RecvTimeoutError::Timeout) => None,
                // This case should trigger upon shutdown, which is why this is a warning rather
                // than an error.
                Err(RecvTimeoutError::Disconnected) => {
                    warn!(
                        "Channel disconnected for generator worker '{}', shutting down.",
                        label
                    );
                    return;
                }
            },
        };

        let Some(cmd) = cmd else { continue };

        let cpos = cmd.pos;

        let cref = match cm.get_loaded_chunk(cpos, true) {
            Ok(cref) => cref,
            // A global lock isn't an error for us, we just need to try again a little later.
            Err(error) => {
                if error.is_globally_locked() {
                    // Place the command in the backlog, we'll try again next loop.
                    backlog_cmd = Some(cmd);
                    // We need to sleep here to emulate waiting on the channel.
                    // Otherwise we end up busy looping.
                    thread::sleep(params.timeout);
                }

                // We don't log the error here because it likely isn't an error. If
                // the chunk didn't exist then it's likely that the chunk was unloaded before
                // we could process the command, which is normal. And other than that
                // there's not really any error that's relevant here other than the global lock error,
                // (which we already handled) so we just skip to the next command.
                continue;
            }
        };

        // We only want to generate into primordial chunks. Generating into already populated chunks
        // is possible but usually undesirable, and if we want to do it we probably don't want to use
        // the worldgen system for it, but rather manually work the generator algorithm.
        if !cref.flags().contains(ChunkFlags::PRIMORDIAL) {
            continue;
        }

        // Flag this chunk as being generated.
        cref.update_flags(|flags| {
            flags.insert(ChunkFlags::GENERATING);
        });

        let result = cref.with_access(true, |mut access| {
            match generator.write_to_chunk(cpos, &mut access) {
                Ok(()) => {
                    // Optimize the chunk a bit before we flag it as updated. This can make
                    // building the mesh for this chunk faster.
                    access.coalesce_microblocks();
                    access.optimize_internal_storage();
                }
                Err(error) => {
                    error!("Generator raised an error generating chunk at {cpos}: {error}")
                }
            }
        });

        if let Err(error) = result {
            error!("Error getting write access to chunk '{cpos}': {error}");
            return;
        }

        // At last we remove both the primordial flag and the generating flag, indicating that
        // this chunk is ready to be treated as any other chunk.
        // We also set the remesh flags here so that the mesh is built.
        cref.update_flags(|flags| {
            flags.remove(ChunkFlags::GENERATING | ChunkFlags::PRIMORDIAL);
            flags.insert(
                ChunkFlags::FRESHLY_GENERATED | ChunkFlags::REMESH_NEIGHBORS | ChunkFlags::REMESH,
            );
        });
    }
}

impl Worker {
    pub fn new(seed: u32, pool: &TaskPool, params: WorkerParams, label: String) -> Self {
        let generator = Generator::new(seed, &params.registries);

        let atomic_interrupt = Arc::new(AtomicBool::new(false));

        let task_label = label.clone();
        let task_interrupt = atomic_interrupt.clone();
        let task = pool.spawn(internal_worker_task(
            generator,
            params,
            task_interrupt,
            task_label,
        ));

        Self {
            task,
            interrupt: atomic_interrupt,
            label,
        }
    }

    pub async fn stop(self) {
        self.interrupt.store(true, Ordering::Relaxed);
        future::poll_once(self.task.cancel()).await;
    }
}

#[derive(Resource)]
pub struct GeneratorWorkerPool {
    workers: Vec<Worker>,
    cmds: Sender<GeneratorCommand>,
}

impl GeneratorWorkerPool {
    pub fn new(
        seed: u32,
        worker_count: usize,
        pool: &TaskPool,
        registries: Registries,
        cm: Arc<ChunkManager>,
    ) -> Self {
        let (cmd_sender, cmd_recver) = channel::unbounded::<GeneratorCommand>();
        let mut workers = Vec::<Worker>::with_capacity(worker_count);

        let default_channel_timeout_duration = Duration::from_millis(500);

        let worker_params = WorkerParams {
            registries,
            chunk_manager: cm,
            cmds: cmd_recver,
            timeout: default_channel_timeout_duration,
        };

        for i in 0..worker_count {
            let worker = Worker::new(
                seed,
                pool,
                worker_params.clone(),
                format!("generator_worker_{i}"),
            );

            workers.push(worker);
        }

        Self {
            workers,
            cmds: cmd_sender,
        }
    }

    pub fn shutdown(self) {
        for worker in self.workers.into_iter() {
            block_on(worker.stop());
        }
    }

    pub fn queue_job(&self, pos: ChunkPos) {
        self.cmds
            .send(GeneratorCommand {
                pos,
                id: GeneratorCommandId::new(0), // TODO: generation task ID tracking
            })
            .unwrap();
    }
}
