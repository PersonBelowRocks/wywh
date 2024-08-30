use std::sync::Arc;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
};
use cb::channel::{Receiver, Sender};
use hb::hash_map::Entry;
use itertools::Itertools;

use parking_lot::lock_api::RwLockUpgradableReadGuard as ReadGuard;

use crate::topo::{
    controller::{LoadChunks, UnloadChunks},
    world::{chunk::ChunkFlags, Chunk},
    worldgen::{generator::GenerateChunk, GenerationPriority},
};

use super::{
    inner_storage::{load_chunks_from_event, purge_chunks_from_event},
    ChunkManager,
};

/// The granularity of the lock in asynchronous chunk lifecycle tasks.
/// Performance may be different with different values.
/// Too high or too low values can be slow and janky, but *should (hopefully)* never cause any actual bugs other than
/// performance issues.
#[derive(Resource, Clone)]
pub struct ChunkLifecycleTaskLockGranularity(pub usize);

/// An ECS resource for the chunk manager.
#[derive(Resource, Deref)]
pub struct ChunkManagerRes(pub Arc<ChunkManager>);

/// Channels for the chunk generation events, which are produced in an async task as events are processed.
#[derive(Resource)]
pub struct GenerationEventChannels {
    pub tx: Sender<GenerateChunk>,
    pub rx: Receiver<GenerateChunk>,
}

impl Default for GenerationEventChannels {
    fn default() -> Self {
        let (tx, rx) = cb::channel::unbounded();

        Self { tx, rx }
    }
}

/// A little buffer for tracking all chunk loading tasks. We can use this to show some diagnostics and whatnot.
/// Or gracefully handling shutdown.
#[derive(Resource, Deref, DerefMut, Default)]
pub struct ChunkLoadTasks(Vec<Task<()>>);

/// A little buffer for tracking all chunk unloading tasks. Very similar to [`ChunkLoadTasks`].
#[derive(Resource, Deref, DerefMut, Default)]
pub struct ChunkPurgeTasks(Vec<Task<()>>);

/// Handle incoming chunk loading events asynchronously by loading them on another thread in a
/// bevy task (spawned in the [`AsyncComputeTaskPool`]). Tasks will send generation events as needed,
/// which will be collected and forwarded to the bevy [`Events`] resource by [`handle_async_generation_events`]
pub fn handle_chunk_load_events_asynchronously(
    cm: Res<ChunkManagerRes>,
    channels: Res<GenerationEventChannels>,
    mut tasks: ResMut<ChunkLoadTasks>,
    mut incoming: ResMut<Events<LoadChunks>>,
    granularity: Res<ChunkLifecycleTaskLockGranularity>,
) {
    let granularity = granularity.0;

    for event in incoming.update_drain() {
        let cm = cm.clone();
        let generate_chunks_tx = channels.tx.clone();

        let task = AsyncComputeTaskPool::get().spawn(async move {
            load_chunks_from_event(cm.as_ref(), event, &generate_chunks_tx, granularity);
        });

        tasks.push(task);
    }
}

/// A system for removing chunk lifecycle tasks from their tracking pools.
pub fn clear_chunk_lifecycle_task_pools(
    mut load_tasks: ResMut<ChunkLoadTasks>,
    mut purge_tasks: ResMut<ChunkPurgeTasks>,
) {
    load_tasks.retain(|task| !task.is_finished());
    purge_tasks.retain(|task| !task.is_finished());
}

/// A system for handling asynchronous chunk generation events sent by chunk loading tasks.
/// Pretty much just forwards the events to their bevy [`Events`] resource.
pub fn handle_async_generation_events(
    channels: Res<GenerationEventChannels>,
    mut generation_events: EventWriter<GenerateChunk>,
) {
    while let Ok(event) = channels.rx.try_recv() {
        generation_events.send(event);
    }
}

/// Handles chunk unload events by asynchronously purging chunks as needed.
pub fn handle_chunk_unload_events_asynchronously(
    cm: Res<ChunkManagerRes>,
    mut tasks: ResMut<ChunkPurgeTasks>,
    mut incoming: ResMut<Events<UnloadChunks>>,
    granularity: Res<ChunkLifecycleTaskLockGranularity>,
) {
    let granularity = granularity.0;

    for event in incoming.update_drain() {
        let cm = cm.clone();

        let task = AsyncComputeTaskPool::get().spawn(async move {
            purge_chunks_from_event(cm.as_ref(), event, granularity);
        });

        tasks.push(task);
    }
}
