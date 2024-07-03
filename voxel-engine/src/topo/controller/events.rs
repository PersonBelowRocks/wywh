use bevy::prelude::*;

use crate::util::ChunkSet;
use crate::{render::lod::LevelOfDetail, topo::world::ChunkPos};

use super::{error::EventPosMismatch, BatchFlags, LoadReasons, LoadshareId};

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

/// These chunks should be loaded for the given reasons.
/// Chunks will be loaded under the provided reasons if they aren't already loaded, or they will
/// receive the given load reasons in addition to their existing ones.
#[derive(Clone, Event, Debug)]
pub struct LoadChunks {
    pub loadshare: LoadshareId,
    pub reasons: LoadReasons,
    pub auto_generate: bool,
    pub chunks: ChunkSet,
}

/// Event triggered when a chunk is loaded. This event is "downstream" from [`LoadChunksEvent`] in that
/// `LoadChunkEvent`'s handler system in the engine also triggers this event. But this event is dispatched
/// AFTER a chunk is loaded, whereas `LoadChunkEvent` is dispatched TO LOAD a chunk.
/// This event is not triggered when load reasons are updated, only when a new chunk is loaded.
#[derive(Copy, Clone, Event, Debug)]
pub struct LoadedChunkEvent {
    pub chunk_pos: ChunkPos,
    pub auto_generate: bool,
}

/// This chunk should be unloaded for the given reasons.
/// Will remove the provided reasons from an already loaded chunk, and if that chunk ends up having
/// no load reasons left it will be unloaded.
#[derive(Clone, Event, Debug)]
pub struct UnloadChunks {
    pub loadshare: LoadshareId,
    pub reasons: LoadReasons,
    pub chunks: ChunkSet,
}

/// Event triggered when a chunk is unloaded. This event is "downstream" from [`UnloadChunksEvent`] in that
/// `UnloadChunkEvent`'s handler system in the engine also triggers this event. But this event is dispatched
/// AFTER a chunk is unloaded, whereas `UnloadChunkEvent` is dispatched TO UNLOAD a chunk.
/// This event is not triggered when load reasons are updated, only when a chunk is unloaded from the manager.
#[derive(Copy, Clone, Event, Debug)]
pub struct UnloadedChunkEvent {
    pub chunk_pos: ChunkPos,
}

#[derive(Clone, Event, Debug)]
pub struct AddBatchChunks(pub ChunkSet);

#[derive(Clone, Event, Debug)]
pub struct RemoveBatchChunks(pub ChunkSet);

///
#[derive(Clone, Event, Debug)]
pub struct RemovedBatchChunks {
    pub chunks: ChunkSet,
    pub batch: Entity,
}

#[derive(Clone, Event, Debug)]
pub struct AddBatch(pub Option<LevelOfDetail>);
// TODO: loadshare remove event
