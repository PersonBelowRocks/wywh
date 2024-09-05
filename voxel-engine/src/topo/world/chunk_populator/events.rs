use crate::topo::world::ChunkPos;
use bevy::prelude::*;

/// Event to be sent to populate the chunk at the given position. The engine will populate the chunk from either
/// the world generator or by reading its data from disk. Sending population events for unloaded chunks is a no-op,
/// but can clog up the event buffers of there's too many of them.
#[derive(Clone, Event, Debug)]
pub struct PopulateChunkEvent {
    /// The position of the chunk to be populated.
    pub chunk_pos: ChunkPos,
    /// The priority of this event. The *higher* the priority, the *sooner* the event will be handled.
    pub priority: u32,
}

/// Event to be sent for recalculating the priorities of other chunk population events,
/// based on the provided strategy. Often the event priority is based on the distance between the chunk
/// and the closest chunk observer.
#[derive(Clone, Debug, Event)]
pub struct RecalculatePopulateEventPriorities {
    pub strategy: PriorityCalcStrategy,
}

/// How the population event priority should be calculated.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum PriorityCalcStrategy {
    /// Priority will be based on the distance to the closest position in the vector.
    /// The closer an event's chunk is to one of these positions, the higher the event's priority.
    /// Usually you want to set the positions here to the positions of all chunk observers.
    ClosestDistance(Vec<Vec3>),
}
