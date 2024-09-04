use bevy::prelude::*;

use crate::topo::world::chunk_manager::ChunkLoadResult;
use crate::util::ChunkSet;
use crate::{render::lod::LevelOfDetail, topo::world::ChunkPos};

use super::{LoadReasons, LoadshareId};

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
    pub chunks: Vec<ChunkPos>,
}

/// Event triggered when a chunk is loaded. This event is "downstream" from [`LoadChunksEvent`] in that
/// `LoadChunkEvent`'s handler system in the engine also triggers this event. But this event is dispatched
/// AFTER a chunk is loaded, whereas `LoadChunkEvent` is dispatched TO LOAD a chunk.
/// This event is not triggered when load reasons are updated, only when a new chunk is loaded.
#[derive(Copy, Clone, Event, Debug)]
pub struct LoadedChunkEvent {
    pub chunk_pos: ChunkPos,
    pub auto_generate: bool,
    pub load_result: ChunkLoadResult,
}

/// Event triggered when the load reasons for a chunk are updated.
#[derive(Copy, Clone, Event, Debug)]
pub struct LoadReasonsAddedEvent {
    pub chunk_pos: ChunkPos,
    /// The load reasons added to the chunk for this loadshare.
    pub reasons_added: LoadReasons,
    /// The load reasons' loadshare.
    pub loadshare: LoadshareId,
    /// Whether the chunk was just loaded and these load reasons were the ones first added.
    pub was_loaded: bool,
}

/// This chunk should be unloaded for the given reasons.
/// Will remove the provided reasons from an already loaded chunk, and if that chunk ends up having
/// no load reasons left it will be unloaded.
#[derive(Clone, Event, Debug)]
pub struct UnloadChunks {
    pub loadshare: LoadshareId,
    pub reasons: LoadReasons,
    pub chunks: Vec<ChunkPos>,
}

/// Event triggered when a chunk is purged. This event is "downstream" of the [`UnloadChunks`] event,
/// because [`UnloadChunks`] events will lead to chunks being purged and this event being sent.
#[derive(Copy, Clone, Event, Debug)]
pub struct PurgedChunkEvent {
    pub chunk_pos: ChunkPos,
}

/// Event triggered when load reasons are removed from a chunk.
#[derive(Copy, Clone, Event, Debug)]
pub struct LoadReasonsRemovedEvent {
    pub chunk_pos: ChunkPos,
    /// The load reasons that were removed from the chunk for this loadshare.
    pub reasons_removed: LoadReasons,
    /// The loadshare that had load reasons removed.
    pub loadshare: LoadshareId,
    /// Whether the removal of the load reasons caused the chunk to be purged.
    pub was_purged: bool,
}

#[derive(Clone, Event, Debug)]
pub struct AddBatchChunks(pub Vec<ChunkPos>);

#[derive(Clone, Event, Debug)]
pub struct RemoveBatchChunks(pub Vec<ChunkPos>);

///
#[derive(Clone, Event, Debug)]
pub struct RemovedBatchChunks {
    pub chunks: Vec<ChunkPos>,
    pub batch: Entity,
}

#[derive(Clone, Event, Debug)]
pub struct AddBatch(pub Option<LevelOfDetail>);

// TODO: loadshare remove event

// TODO: docs
#[derive(Clone, Event, Debug)]
pub struct PopulateChunkEvent {
    pub chunk_pos: ChunkPos,
    pub priority: u32,
}
