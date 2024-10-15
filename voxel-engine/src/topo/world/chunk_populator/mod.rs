pub mod default_generator;
pub mod error;
pub mod events;
pub mod worldgen;

use std::{hash::BuildHasher, sync::Arc};

use async_bevy_events::{AsyncEventPlugin, AsyncEventReader, EventFunnel, EventFunnelPlugin};
use bevy::{
    prelude::*,
    tasks::{available_parallelism, block_on, AsyncComputeTaskPool, Task, TaskPoolBuilder},
};
use default_generator::WorldGenerator;
use events::{
    ChunkPopulated, PopulateChunk, PriorityCalcStrategy, RecalculatePopulateEventPrioritiesEvent,
};
use futures::StreamExt;
use priority_queue::PriorityQueue;
use worldgen::{
    WorldgenWorker, WorldgenWorkerPool, WORLDGEN_TASK_POOL, WORLDGEN_TASK_POOL_THREAD_NAME,
};

use crate::{
    data::registries::Registries,
    util::{closest_distance, closest_distance_sq},
    CoreEngineSetup, EngineState,
};

use super::{ChunkManager, ChunkPos, VoxelRealm};

pub struct ChunkPopulatorController;

impl Plugin for ChunkPopulatorController {
    fn build(&self, app: &mut App) {
        info!("Setting up chunk populator controller");

        WORLDGEN_TASK_POOL.set(
            TaskPoolBuilder::new()
                .num_threads(available_parallelism() / 2)
                .thread_name(WORLDGEN_TASK_POOL_THREAD_NAME.into())
                .build(),
        ).expect("build() should only be called once, and it's the only place where we initialize the pool");

        app.add_plugins((
            AsyncEventPlugin::<RecalculatePopulateEventPrioritiesEvent>::default(),
            AsyncEventPlugin::<PopulateChunk>::default(),
            EventFunnelPlugin::<ChunkPopulated>::for_new(),
        ));

        app.add_systems(
            OnEnter(EngineState::Finished),
            start_chunk_population_event_bus_task.in_set(CoreEngineSetup::Initialize),
        );
    }
}

/// Task responsible for receiving chunk population events and passing them on to
/// be handled appropriately. Chunks are either populated from disk or generated.
/// This task is responsible for deciding which of the two options should be chosen.
#[derive(Resource)]
pub struct PopulatorTaskHandle {
    shutdown_tx: flume::Sender<()>,
    task: Option<Task<()>>,
}

impl Drop for PopulatorTaskHandle {
    fn drop(&mut self) {
        if self.shutdown_tx.send(()).is_err() {
            warn!("Shutdown channel for chunk populator task was disconnected");
        }

        block_on(self.task.take().unwrap())
    }
}

pub struct PopulatorTaskState {
    chunk_manager: Arc<ChunkManager>,
    registries: Registries,
    worldgen_worker_pool: WorldgenWorkerPool,
}

/// Calculate new priorities for the items in the queue based on the distance returned by the distance function.
/// The distance function's argument is the worldspace center position of a chunk.
/// The priority will be higher the smaller the value returned by the distance function.
/// Formula for the priority is roughly `u32::MAX - distance_fn(chunk_pos)`.
#[inline]
fn calculate_priorities_based_on_distance<H: BuildHasher, F: Fn(Vec3) -> f32>(
    distance_fn: F,
    queue: &mut PriorityQueue<ChunkPos, u32, H>,
) {
    for (&mut chunk_pos, priority) in queue.iter_mut() {
        let center = chunk_pos.worldspace_center();
        let min_distance = distance_fn(center);

        // Closer chunk positions are higher priority, so we need to invert the distance.
        *priority = u32::MAX - (min_distance as u32);
    }
}

impl PopulatorTaskState {
    pub fn new<F: Fn() -> Box<dyn WorldgenWorker>>(
        chunk_manager: Arc<ChunkManager>,
        registries: Registries,
        chunk_populated_funnel: EventFunnel<ChunkPopulated>,
        worldgen_worker_factory: F,
    ) -> Self {
        let worldgen_worker_pool = WorldgenWorkerPool::new(
            chunk_populated_funnel,
            chunk_manager.clone(),
            worldgen_worker_factory,
        );

        Self {
            chunk_manager,
            registries,
            worldgen_worker_pool,
        }
    }

    pub fn handle_populate_chunk_event(&mut self, event: PopulateChunk) {
        self.worldgen_worker_pool
            .job_queue
            .lock()
            .push(event.chunk_pos, event.priority);
    }

    pub fn handle_recalc_priorities_event(
        &mut self,
        event: RecalculatePopulateEventPrioritiesEvent,
    ) {
        let mut guard = self.worldgen_worker_pool.job_queue.lock();

        match event.strategy {
            PriorityCalcStrategy::ClosestDistanceSq(positions) => {
                calculate_priorities_based_on_distance(
                    |chunk_center| {
                        closest_distance_sq(chunk_center, positions.iter().cloned()).unwrap_or(0.0)
                    },
                    &mut guard,
                );
            }
            PriorityCalcStrategy::ClosestDistance(positions) => {
                calculate_priorities_based_on_distance(
                    |chunk_center| {
                        closest_distance(chunk_center, positions.iter().cloned()).unwrap_or(0.0)
                    },
                    &mut guard,
                );
            }
        }
    }

    pub async fn on_shutdown(&mut self) {
        self.worldgen_worker_pool.shutdown().await;
    }
}

/// This system starts the [`PopulationEventBusTask`] task with the appropriate data and configuration.
pub fn start_chunk_population_event_bus_task(
    mut cmds: Commands,
    realm: VoxelRealm,
    registries: Res<Registries>,
    populate_chunk_events: Res<AsyncEventReader<PopulateChunk>>,
    recalc_priority_events: Res<AsyncEventReader<RecalculatePopulateEventPrioritiesEvent>>,
    chunk_populated_funnel: Res<EventFunnel<ChunkPopulated>>,
) {
    info!("Starting background chunk population event bus task.");

    let populate_chunk_events = populate_chunk_events.clone();
    let recalc_priority_events = recalc_priority_events.clone();

    let mut task_state = PopulatorTaskState::new(
        realm.clone_cm(),
        registries.clone(),
        chunk_populated_funnel.clone(),
        || Box::new(WorldGenerator::new(&registries)),
    );

    // This is basically a oneshot channel. If we send a message through here we tell the chunk populator task to shut down.
    let (shutdown_tx, shutdown_rx) = flume::bounded::<()>(1);

    let task = AsyncComputeTaskPool::get().spawn(async move {
        let mut populate_chunk_events_stream = populate_chunk_events.stream();
        let mut recalc_priority_events_stream = recalc_priority_events.stream();
        let mut shutdown_stream = shutdown_rx.stream();

        'task_loop: loop {
            futures::select! {
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

    cmds.insert_resource(PopulatorTaskHandle {
        shutdown_tx,
        task: Some(task),
    });
}
