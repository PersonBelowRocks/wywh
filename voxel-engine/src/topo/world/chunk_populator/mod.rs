pub mod error;
pub mod events;
pub mod worldgen;

use std::sync::Arc;

use async_bevy_events::{AsyncEventPlugin, AsyncEventReader};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, TaskPool},
};
use events::{PopulateChunkEvent, RecalculatePopulateEventPriorities};
use futures_util::StreamExt;
use worldgen::{WorldgenTaskPool, WorldgenWorker, WorldgenWorkerPool};

use crate::data::registries::Registries;

use super::{ChunkManager, VoxelRealm};

pub struct ChunkPopulatorController;

impl Plugin for ChunkPopulatorController {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            AsyncEventPlugin::<RecalculatePopulateEventPriorities>::default(),
            AsyncEventPlugin::<PopulateChunkEvent>::default(),
        ));

        todo!()
    }
}

/// Task responsible for receiving chunk population events and passing them on to
/// be handled appropriately. Chunks are either populated from disk or generated.
/// This task is responsible for deciding which of the two options should be chosen.
#[derive(Resource)]
pub struct PopulatorTaskHandle {
    shutdown_tx: flume::Sender<()>,
    task: Task<()>,
}

pub struct PopulatorTaskState {
    chunk_manager: Arc<ChunkManager>,
    registries: Registries,
    worldgen_task_pool: Arc<TaskPool>,
    worldgen_worker_pool: WorldgenWorkerPool,
}

impl PopulatorTaskState {
    pub fn new<F: Fn() -> Box<dyn WorldgenWorker>>(
        chunk_manager: Arc<ChunkManager>,
        registries: Registries,
        worldgen_task_pool: Arc<TaskPool>,
        worldgen_worker_factory: F,
    ) -> Self {
        let worldgen_worker_pool =
            WorldgenWorkerPool::new(&worldgen_task_pool, worldgen_worker_factory);

        Self {
            chunk_manager,
            registries,
            worldgen_task_pool,
            worldgen_worker_pool,
        }
    }

    pub fn handle_populate_chunk_event(&mut self, event: PopulateChunkEvent) {
        self.worldgen_worker_pool
            .job_queue
            .lock()
            .push(event.chunk_pos, event.priority);
    }

    pub fn handle_recalc_priorities_event(&mut self, event: RecalculatePopulateEventPriorities) {
        let mut guard = self.worldgen_worker_pool.job_queue.lock();

        for (&mut chunk_pos, priority) in guard.iter_mut() {
            todo!();
        }
    }

    pub async fn on_shutdown(&mut self) {
        self.worldgen_worker_pool.shutdown().await;
    }
}

/// This system starts the [`PopulationEventBusTask`] task with the appropriate data and configuration.
pub fn start_chunk_population_event_bus_task(
    mut cmds: Commands,
    worldgen_task_pool: Res<WorldgenTaskPool>,
    realm: VoxelRealm,
    registries: Res<Registries>,
    populate_chunk_events: Res<AsyncEventReader<PopulateChunkEvent>>,
    recalc_priority_events: Res<AsyncEventReader<RecalculatePopulateEventPriorities>>,
) {
    let populate_chunk_events = populate_chunk_events.clone();
    let recalc_priority_events = recalc_priority_events.clone();

    let mut task_state = PopulatorTaskState::new(
        realm.clone_cm(),
        registries.clone(),
        worldgen_task_pool.clone(),
        || todo!(),
    );

    // This is basically a oneshot channel. If we send a message through here we tell the chunk populator task to shut down.
    let (shutdown_tx, shutdown_rx) = flume::bounded::<()>(1);

    let task = AsyncComputeTaskPool::get().spawn(async move {
        let mut populate_chunk_events_stream = populate_chunk_events.stream();
        let mut recalc_priority_events_stream = recalc_priority_events.stream();
        let mut shutdown_stream = shutdown_rx.stream();

        'task_loop: loop {
            futures_util::select! {
                // If we ever receive something on the shutdown channel, we stop the task.
                _ = shutdown_stream.next() => {
                    break 'task_loop;
                },
                event = populate_chunk_events_stream.next() => {
                    let Some(event) = event else {
                        continue;
                    };

                    task_state.handle_populate_chunk_event(event);
                },
                event = recalc_priority_events_stream.next() => {
                    let Some(event) = event else {
                        continue;
                    };

                    task_state.handle_recalc_priorities_event(event);
                }
            }
        }

        task_state.on_shutdown().await;
    });

    cmds.insert_resource(PopulatorTaskHandle { shutdown_tx, task });
}
