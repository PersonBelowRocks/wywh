use bevy::prelude::*;

use crate::topo::world::ChunkPos;
use crate::topo::world::chunk_manager::ChunkLoadResult;

#[derive(Clone, Event, Debug)]
pub struct CrossChunkBorder {
    /// Indicates if this observer entity was just inserted.
    /// i.e. instead of a regular movement where its current chunk was different from its previous chunk,
    /// this movement event was because the entity didn't even have a previous chunk position,
    /// and this is the first time we recorded its chunk position.
    pub new: bool,
    pub old_chunk: ChunkPos,
    pub new_chunk: ChunkPos,
}

/// Load these chunks for the given `actor`.
#[derive(Clone, Event, Debug)]
pub struct LoadChunksEvent {
    pub actor: Entity,
    /// Whether the chunks should be populated by the chunk populator.
    pub auto_populate: bool,
    pub chunks: Vec<ChunkPos>,
}

/// Event triggered when a chunk is loaded. This event is "downstream" from [`LoadChunksEvent`] in that
/// `LoadChunkEvent`'s handler system in the engine also sends this event. But this event is dispatched
/// AFTER a chunk is loaded, whereas [`LoadChunksEvent`] is dispatched TO LOAD chunks.
/// This event is not triggered when an already loaded chunk is loaded by another actor, only when a new chunk is loaded.
#[derive(Copy, Clone, Event, Debug)]
pub struct LoadedChunkEvent {
    pub chunk_pos: ChunkPos,
    /// Whether the chunks should be populated by the chunk populator.
    pub auto_populate: bool,
    pub load_result: ChunkLoadResult,
}

/// These chunks should be unloaded for the given actor.
/// If the chunk is not loaded by any actors after this event is handled, it will be purged.
#[derive(Clone, Event, Debug)]
pub struct UnloadChunksEvent {
    pub actor: Entity,
    pub chunks: Vec<ChunkPos>,
}

/// Event triggered when a chunk is purged. This event is "downstream" of the [`UnloadChunks`] event,
/// because [`UnloadChunks`] events will lead to chunks being purged and this event being sent.
#[derive(Copy, Clone, Event, Debug)]
pub struct PurgedChunkEvent {
    pub chunk_pos: ChunkPos,
}

// TODO: loadshare remove event
