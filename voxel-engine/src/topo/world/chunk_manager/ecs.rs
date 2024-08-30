use std::{sync::Arc, thread::JoinHandle};

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

/// Tasks for managing the lifecycle of chunks, and channels to send commands to those tasks.
#[derive(Resource, Default)]
pub struct ChunkLifecycleTasks {
    load: Option<JoinHandle<()>>,
    purge: Option<JoinHandle<()>>,
}

impl ChunkLifecycleTasks {
    /// Initialize and start the chunk loading task.
    ///
    /// # Panics
    /// Will panic if the task is already initialized.
    pub fn init_load_task(
        &mut self,
        cm: Arc<ChunkManager>,
        granularity: usize,
        generate_chunks_tx: Sender<GenerateChunk>,
    ) -> Sender<LoadChunks> {
        if self.load.is_some() {
            panic!("Chunk loading task is already initialized");
        }

        let (tx, rx) = cb::channel::unbounded::<LoadChunks>();
        let load_task_handle = std::thread::spawn(move || {
            while let Ok(incoming) = rx.recv() {
                load_chunks_from_event(cm.as_ref(), incoming, &generate_chunks_tx, granularity);
            }
        });

        self.load = Some(load_task_handle);

        tx
    }

    /// Initialize and start the chunk purging task.
    ///
    /// # Panics
    /// Will panic if the task is already initialized.
    pub fn init_purge_task(
        &mut self,
        cm: Arc<ChunkManager>,
        granularity: usize,
    ) -> Sender<UnloadChunks> {
        if self.purge.is_some() {
            panic!("Chunk purging task is already initialized");
        }

        let (tx, rx) = cb::channel::unbounded::<UnloadChunks>();
        let purge_task_handle = std::thread::spawn(move || {
            while let Ok(incoming) = rx.recv() {
                purge_chunks_from_event(cm.as_ref(), incoming, granularity);
            }
        });

        self.purge = Some(purge_task_handle);

        tx
    }
}

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

/// Handle incoming chunk loading events asynchronously by loading them on another thread.
/// Tasks will send generation events as needed, which will be collected and
/// forwarded to the bevy [`Events`] resource by [`handle_async_generation_events`]
pub fn handle_chunk_load_events_asynchronously(
    cm: Res<ChunkManagerRes>,
    channels: Res<GenerationEventChannels>,
    mut tasks: ResMut<ChunkLifecycleTasks>,
    mut incoming: ResMut<Events<LoadChunks>>,
    mut task_event_sender: Local<Option<Sender<LoadChunks>>>,
    granularity: Res<ChunkLifecycleTaskLockGranularity>,
) {
    let granularity = granularity.0;

    let event_sender = task_event_sender
        .get_or_insert_with(|| tasks.init_load_task(cm.clone(), granularity, channels.tx.clone()));

    for event in incoming.update_drain() {
        event_sender.send(event).unwrap();
    }
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
    mut tasks: ResMut<ChunkLifecycleTasks>,
    mut incoming: ResMut<Events<UnloadChunks>>,
    mut task_event_sender: Local<Option<Sender<UnloadChunks>>>,
    granularity: Res<ChunkLifecycleTaskLockGranularity>,
) {
    let granularity = granularity.0;
    let event_sender =
        task_event_sender.get_or_insert_with(|| tasks.init_purge_task(cm.clone(), granularity));

    for event in incoming.update_drain() {
        event_sender.send(event).unwrap();
    }
}
