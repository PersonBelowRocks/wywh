use std::time::Duration;

use bevy::prelude::*;
use bitflags::bitflags;
use error::EventPosMismatch;
use handle_events::{handle_chunk_loads_and_unloads, handle_permit_updates};
use observer_events::{dispatch_move_events, load_in_range_chunks, unload_out_of_range_chunks};

use crate::EngineState;

use super::world::ChunkPos;

mod error;
mod events;
mod handle_events;
mod observer_events;
mod permits;
pub use events::*;

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

/// System sets for the world controller
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, SystemSet)]
pub enum WorldControllerSystems {
    CoreEvents,
    ObserverMovement,
    ObserverResponses,
}

#[derive(Copy, Clone, Resource, Debug)]
pub struct WorldControllerSettings {
    pub chunk_loading_handler_timeout: Duration,
    pub chunk_loading_max_stalling: Duration,
    pub chunk_loading_handler_backlog_threshold: usize,
}

pub struct WorldController {
    pub settings: WorldControllerSettings,
}

impl Plugin for WorldController {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.settings)
            .add_event::<LoadChunkEvent>()
            .add_event::<UnloadChunkEvent>()
            .add_event::<UpdatePermitEvent>()
            .add_event::<ChunkObserverMoveEvent>()
            .add_event::<ChunkObserverCrossChunkBorderEvent>();

        app.add_systems(
            FixedPostUpdate,
            (
                dispatch_move_events.in_set(WorldControllerSystems::ObserverMovement),
                (unload_out_of_range_chunks, load_in_range_chunks)
                    .chain()
                    .in_set(WorldControllerSystems::ObserverResponses),
                (handle_chunk_loads_and_unloads, handle_permit_updates)
                    .chain()
                    .in_set(WorldControllerSystems::CoreEvents),
            ),
        );

        app.configure_sets(
            FixedPostUpdate,
            (
                WorldControllerSystems::ObserverMovement,
                WorldControllerSystems::ObserverResponses,
                WorldControllerSystems::CoreEvents,
            )
                .chain()
                .run_if(in_state(EngineState::Finished)),
        );
    }
}
