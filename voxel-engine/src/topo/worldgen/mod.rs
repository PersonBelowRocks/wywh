use std::{
    cmp,
    collections::BinaryHeap,
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
    util::{sync::LockStrategy, Keyed, KeyedOrd},
};

use self::generator::Generator;

use super::world::{chunk::ChunkFlags, ChunkManager, ChunkPos};

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenerationPriority(u32);

impl GenerationPriority {
    pub const HIGHEST: Self = Self(0);
    pub const LOWEST: Self = Self(u32::MAX);

    pub fn new(raw: u32) -> Self {
        Self(raw)
    }
}

impl PartialOrd for GenerationPriority {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GenerationPriority {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        other.0.cmp(&self.0)
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, dm::Constructor)]
pub struct GeneratorCommandId(u32);

#[derive(Clone)]
pub struct GeneratorCommand {
    pub pos: ChunkPos,
    pub priority: GenerationPriority,
}

impl Keyed<GenerationPriority> for GeneratorCommand {
    type Key = GenerationPriority;

    fn key(&self) -> &Self::Key {
        &self.priority
    }
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
        if !cref
            .flags(LockStrategy::Blocking)
            .unwrap()
            .contains(ChunkFlags::PRIMORDIAL)
        {
            continue;
        }

        // Flag this chunk as being generated.
        cref.update_flags(LockStrategy::Blocking, |flags| {
            flags.insert(ChunkFlags::GENERATING);
        })
        .unwrap();

        let mut handle = cref.chunk().write_handle(LockStrategy::Blocking).unwrap();

        match generator.write_to_chunk(cpos, &mut handle) {
            Ok(()) => {
                // TODO: run cleanup routine for the chunk
            }
            Err(error) => {
                error!("Generator raised an error generating chunk at {cpos}: {error}")
            }
        }

        // Need to drop the handle to free the lock.
        drop(handle);

        // At last we remove both the primordial flag and the generating flag, indicating that
        // this chunk is ready to be treated as any other chunk.
        // We also set the remesh flags here so that the mesh is built.
        cref.update_flags(LockStrategy::Blocking, |flags| {
            flags.remove(ChunkFlags::GENERATING | ChunkFlags::PRIMORDIAL);
            flags.insert(
                ChunkFlags::FRESHLY_GENERATED | ChunkFlags::REMESH_NEIGHBORS | ChunkFlags::REMESH,
            );
        })
        .unwrap();
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

#[derive(Copy, Clone)]
pub struct GeneratorPoolSettings {
    pub workers: usize,
    pub job_channel_capacity: usize,
}

#[derive(Resource)]
pub struct GeneratorWorkerPool {
    workers: Vec<Worker>,
    cmds: Sender<GeneratorCommand>,
    pending: BinaryHeap<KeyedOrd<GeneratorCommand, GenerationPriority>>,
}

impl GeneratorWorkerPool {
    pub fn new(
        settings: GeneratorPoolSettings,
        seed: u32,
        pool: &TaskPool,
        registries: Registries,
        cm: Arc<ChunkManager>,
    ) -> Self {
        let (cmd_sender, cmd_recver) =
            channel::bounded::<GeneratorCommand>(settings.job_channel_capacity);
        let mut workers = Vec::<Worker>::with_capacity(settings.workers);

        let default_channel_timeout_duration = Duration::from_millis(50);

        let worker_params = WorkerParams {
            registries,
            chunk_manager: cm,
            cmds: cmd_recver,
            timeout: default_channel_timeout_duration,
        };

        for i in 0..settings.workers {
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
            pending: BinaryHeap::new(),
        }
    }

    pub fn shutdown(self) {
        for worker in self.workers.into_iter() {
            block_on(worker.stop());
        }
    }

    pub fn queue_jobs<I: Iterator<Item = GeneratorCommand>>(&mut self, cmds: I) {
        self.pending.extend(cmds.map(KeyedOrd::new));

        while let Some(next) = self.pending.pop() {
            let next = next.into_inner();

            if let Err(error) = self.cmds.try_send(next) {
                match error {
                    TrySendError::Disconnected(msg) => {
                        self.pending.push(KeyedOrd::new(msg));
                        error!("Could not send generator command to workers because the channel is disconnected.");
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
}
