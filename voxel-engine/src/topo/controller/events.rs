use bevy::prelude::*;

use crate::topo::world::ChunkPos;

use super::{error::EventPosMismatch, LoadReasons, PermitFlags};

pub(super) trait MergeEvent: Sized {
    fn pos(&self) -> ChunkPos;
    fn merge(&mut self, other: Self) -> Result<(), EventPosMismatch>;
}

#[derive(Clone, Event, Debug)]
pub struct ChunkObserverMoveEvent {
    /// Indicates if this observer entity was just inserted.
    /// i.e. instead of a regular movement where its current position was different from its last position,
    /// this movement event was because the entity didn't even have a last position, and this was the first time
    /// we recorded its position.
    pub new: bool,
    pub entity: Entity,
    pub old_pos: Vec3,
    pub new_pos: Vec3,
}

#[derive(Clone, Event, Debug)]
pub struct ChunkObserverCrossChunkBorderEvent {
    /// Same as for ChunkObserverMoveEvent.
    pub new: bool,
    pub entity: Entity,
    pub old_chunk: ChunkPos,
    pub new_chunk: ChunkPos,
}

/// This chunk should be loaded for the given reasons.
/// Will either load a chunk with the provided reasons, or add the given load reasons to an
/// already loaded chunk.
#[derive(Copy, Clone, Event, Debug)]
pub struct LoadChunkEvent {
    pub chunk_pos: ChunkPos,
    pub reasons: LoadReasons,
    pub auto_generate: bool,
}

impl MergeEvent for LoadChunkEvent {
    fn pos(&self) -> ChunkPos {
        self.chunk_pos
    }

    fn merge(&mut self, other: Self) -> Result<(), EventPosMismatch> {
        if self.chunk_pos != other.chunk_pos {
            Err(EventPosMismatch {
                this: self.chunk_pos,
                other: other.chunk_pos,
            })
        } else {
            self.auto_generate |= other.auto_generate;
            self.reasons |= other.reasons;

            Ok(())
        }
    }
}

/// This chunk should be unloaded for the given reasons.
/// Will remove the provided reasons from an already loaded chunk, and if that chunk ends up having
/// no load reasons left it will be unloaded.
#[derive(Copy, Clone, Event, Debug)]
pub struct UnloadChunkEvent {
    pub chunk_pos: ChunkPos,
    pub reasons: LoadReasons,
}

impl MergeEvent for UnloadChunkEvent {
    fn pos(&self) -> ChunkPos {
        self.chunk_pos
    }

    fn merge(&mut self, other: Self) -> Result<(), EventPosMismatch> {
        if self.chunk_pos != other.chunk_pos {
            Err(EventPosMismatch {
                this: self.chunk_pos,
                other: other.chunk_pos,
            })
        } else {
            self.reasons |= other.reasons;

            Ok(())
        }
    }
}

#[derive(Copy, Clone, Event, Debug)]
pub struct UpdatePermitEvent {
    pub chunk_pos: ChunkPos,
    pub insert_flags: PermitFlags,
    pub remove_flags: PermitFlags,
}
