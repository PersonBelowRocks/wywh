use crate::topo::world::ChunkPos;
use bevy::prelude::*;

// TODO: docs
#[derive(Clone, Event, Debug)]
pub struct PopulateChunkEvent {
    pub chunk_pos: ChunkPos,
    pub priority: u32,
}
