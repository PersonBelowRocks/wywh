use std::sync::Arc;

use bevy::{
    ecs::event::ManualEventReader,
    prelude::*,
    tasks::{futures_lite::FutureExt, AsyncComputeTaskPool, Task},
};
use cb::channel::{Receiver, Sender};
use itertools::Itertools;

use crate::topo::{
    controller::LoadChunks,
    world::{chunk::ChunkFlags, Chunk},
    worldgen::{generator::GenerateChunk, GenerationPriority},
};

use super::ChunkManager2;

/// An ECS resource for the chunk manager.
#[derive(Resource, Deref)]
pub struct ChunkManagerRes(Arc<ChunkManager2>);

/// Channels for the chunk generation events, which are produced in an async task as events are processed.
#[derive(Resource)]
pub struct GenerationEventChannels {
    pub tx: Sender<GenerateChunk>,
    pub rx: Receiver<GenerateChunk>,
}

impl GenerationEventChannels {
    pub fn new() -> Self {
        let (tx, rx) = cb::channel::unbounded();

        Self { tx, rx }
    }
}

/// A little buffer for tracking all chunk loading tasks. We can use this to show some diagnostics and whatnot.
/// Or gracefully handling shutdown.
#[derive(Resource, Deref, DerefMut, Default)]
pub struct ChunkLoadTasks(Vec<Task<()>>);

/// Load chunks from a [`LoadChunks`] event.
fn load_chunks_from_event(cm: &ChunkManager2, event: LoadChunks) -> Option<Vec<GenerateChunk>> {
    // Only pre-allocate if we're asked to send generation events,
    // otherwise we're just doing a pointless allocation
    let mut generation_events = if event.auto_generate {
        Vec::with_capacity(event.chunks.len())
    } else {
        Vec::new()
    };

    for chunk_pos in event.chunks.iter() {
        let mut send_generation_event = event.auto_generate;

        if cm.storage.is_loaded(chunk_pos) {
            // Chunk is already loaded so we update the load reasons.
            cm.storage.add_load_reasons(chunk_pos, event.reasons);
        } else {
            let chunk = Chunk::new(chunk_pos, cm.default_block, ChunkFlags::PRIMORDIAL);

            if let Err(error) = cm.storage.load(chunk, event.reasons) {
                // Don't send generation events if we had an error when loading the chunk, try
                // to keep the errors confined and don't let the rest of the engine touch potentially
                // broken chunks.
                send_generation_event = false;
                // TODO: maybe warn on some errors, and error on others.
                error!("Error loading chunk: {error}");
            }
        }

        if send_generation_event {
            generation_events.push(GenerateChunk {
                chunk_pos,
                // TODO: calculate based on distance to something
                priority: GenerationPriority::new(0),
            })
        }
    }

    // Nothing will happen if there are no generation events so to keep things consistent we
    // just return `None` in that case to signal that there's nothing to do.
    if generation_events.is_empty() {
        None
    } else {
        Some(generation_events)
    }
}

/// Handle incoming chunk loading events asynchronously by loading them on another thread in a
/// bevy task (spawned in the [`AsyncComputeTaskPool`]). Tasks will send generation events as needed,
/// which will be collected and forwarded to the bevy [`Events`] resource by [`handle_async_generation_events`]
pub fn handle_chunk_load_events_asynchronously(
    cm: Res<ChunkManagerRes>,
    channels: Res<GenerationEventChannels>,
    mut tasks: ResMut<ChunkLoadTasks>,
    mut incoming: EventReader<LoadChunks>,
) {
    for event in incoming.read().cloned() {
        let cm = cm.clone();
        let generate_chunks_tx = channels.tx.clone();

        let task = AsyncComputeTaskPool::get().spawn(async move {
            load_chunks_from_event(cm.as_ref(), event)
                .map(Vec::into_iter)
                .map(|events| {
                    events
                        .for_each(|generate_chunk| generate_chunks_tx.send(generate_chunk).unwrap())
                });
        });

        tasks.push(task);
    }
}

/// A system for removing chunk loading tasks from the tracking pool.
pub fn clear_chunk_loading_task_pool(mut tasks: ResMut<ChunkLoadTasks>) {
    tasks.retain(|task| !task.is_finished());
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
