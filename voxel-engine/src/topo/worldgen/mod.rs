use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
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

use super::{chunk::ChunkPos, realm::ChunkManager};

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

fn generate_chunk(generator: &Generator, cmd: GeneratorCommand, cm: &ChunkManager) {
    let cpos = cmd.pos;

    match cm.initialize_new_chunk(cpos) {
        Ok(cref) => {
            let access_result = cref.with_access(|mut access| {
                let gen_result = generator.write_to_chunk(cpos, &mut access);

                if let Err(error) = gen_result {
                    error!("Generator raised an error generating chunk '{cpos}': {error}")
                } else {
                    access.coalesce_microblocks();
                    access.optimize_internal_storage();
                }
            });

            if let Err(error) = access_result {
                error!("Error getting write access to chunk '{cpos}': {error}");
            }
        }
        Err(error) => error!("Error trying to generate chunk at '{cpos}': {error}"),
    }
}

async fn internal_worker_task(
    generator: Generator,
    params: WorkerParams,
    interrupt: Arc<AtomicBool>,
    label: String,
) {
    while !interrupt.load(Ordering::Relaxed) {
        match params.cmds.recv_timeout(params.timeout) {
            Ok(cmd) => {
                generate_chunk(&generator, cmd, &params.chunk_manager);
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                warn!(
                    "Channel disconnected for generator worker '{}', worker is shutting down.",
                    label
                );
                return;
            }
        }
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
