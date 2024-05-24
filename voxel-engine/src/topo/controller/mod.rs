use bevy::prelude::*;
use bitflags::bitflags;

use super::world::ChunkPos;

mod ecs;
mod handle_events;
mod observer_events;
mod permits;

pub use permits::*;

#[derive(Clone, Component, Debug)]
pub struct ChunkObserver {
    pub horizontal_range: f32,
    pub view_distance_above: f32,
    pub view_distance_below: f32,
}

#[derive(Clone, Component, Debug)]
pub struct LastPosition {
    pub ws_pos: Vec3,
    pub chunk_pos: ChunkPos,
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

/// This chunk should be unloaded for the given reasons.
/// Will remove the provided reasons from an already loaded chunk, and if that chunk ends up having
/// no load reasons left it will be unloaded.
#[derive(Copy, Clone, Event, Debug)]
pub struct UnloadChunkEvent {
    pub chunk_pos: ChunkPos,
    pub reasons: LoadReasons,
}

#[derive(Clone, Event, Debug)]
pub struct UpdatePermit {
    pub chunk_pos: ChunkPos,
    pub new_permit: Permit,
}

bitflags! {
    /// Describes reasons for why a chunk should be kept loaded. If a chunk has no load reason flags
    /// set it will eventually be automatically unloaded (and its resources freed).
    /// Removing certain flags can result in some relevant resources being unloaded, but not the chunk
    /// itself (unless all flags are removed). For example if a chunk doesn't have a RENDER flag, it
    /// should not have any associated data on the GPU.
    #[derive(Copy, Clone, Eq, Debug, PartialEq, Hash)]
    pub struct LoadReasons: u16 {
        /// This chunk is manually loaded, and thus should be manually unloaded (aka. force loaded)
        /// The engine won't touch this flag, so it's up to the user to manage force loaded chunks
        const MANUAL = 1 << 0;
        /// This chunk is loaded because it should be rendered, if it passes out of render distance
        /// then this flag will be removed
        const RENDER = 1 << 1;
        /// This chunk is loaded because it should have collisions, if it passes out of physics distance
        /// then this flag will be removed
        const COLLISION = 1 << 2;
    }
}

pub struct WorldController;

impl Plugin for WorldController {
    fn build(&self, app: &mut App) {
        todo!()
    }
}
