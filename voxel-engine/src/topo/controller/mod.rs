use std::convert::identity;
use std::sync::atomic::AtomicBool;
use std::{fmt, time::Duration};

use bevy::{
    prelude::*,
    render::extract_component::{ExtractComponent, ExtractComponentPlugin},
};
use bitflags::bitflags;
use enum_map::EnumMap;
use hb::HashSet;

use handle_events::{
    handle_chunk_loads_and_unloads, handle_permit_flag_additions, handle_permit_flag_removals,
};
use observer_events::{
    dispatch_move_events, generate_chunks_with_priority, load_in_range_chunks,
    unload_out_of_range_chunks,
};

use crate::render::LevelOfDetail;
use crate::{util::ChunkSet, EngineState};

use super::world::ChunkPos;

mod error;
mod events;
mod handle_events;
mod observer_events;
mod permits;
pub use events::*;

use crate::topo::controller::observer_events::grant_observer_loadshares;
pub use permits::*;

#[derive(Clone, Component, Debug)]
pub struct ObserverSettings {
    pub horizontal_range: f32,
    pub view_distance_above: f32,
    pub view_distance_below: f32,
}

impl Default for ObserverSettings {
    fn default() -> Self {
        Self {
            horizontal_range: 4.0,
            view_distance_above: 2.0,
            view_distance_below: 2.0,
        }
    }
}

#[derive(Component)]
pub struct RenderableObserverChunks {
    pub should_extract: AtomicBool,
    pub in_range: EnumMap<LevelOfDetail, Option<ChunkSet>>,
}

impl RenderableObserverChunks {
    pub fn in_range(&self) -> impl Iterator<Item = (LevelOfDetail, &ChunkSet)> + '_ {
        self.in_range
            .iter()
            .filter_map(|(lod, option)| option.as_ref().map(|chunks| (lod, chunks)))
    }
}

impl Default for RenderableObserverChunks {
    fn default() -> Self {
        Self {
            should_extract: AtomicBool::new(true),
            in_range: EnumMap::default(),
        }
    }
}

/// How an observer should treat its loadshare
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum ObserverLoadshareType {
    /// Automatically create a new and unique loadshare for this observer. Upon having one created
    /// the loadshare type will be updated to `ObserverLoadshareType::Observer` with the created
    /// loadshare ID
    #[default]
    Auto,
    /// Manually given loadshare to the observer. In this case the observer shouldn't clear the
    /// entire loadshare if it's removed.
    Manual(LoadshareId),
    /// The loadshare is owned and controlled by the observer, this variant should never be constructed
    /// manually and should only be created from the engine giving an `Auto` observer a loadshare.
    Observer(LoadshareId),
}

/// The loadshare of an observer.
#[derive(Default, Component, Copy, Clone, Debug, PartialEq, Eq)]
pub struct ObserverLoadshare(pub(crate) ObserverLoadshareType);

impl ObserverLoadshare {
    /// Manually track the given loadshare ID. The observer will not clear the entire loadshare if
    /// it's removed
    pub fn manual(id: LoadshareId) -> Self {
        Self(ObserverLoadshareType::Manual(id))
    }

    /// Automatically grant a unique loadshare to this observer and clear the entire loadshare if
    /// the observer is removed.
    pub fn auto() -> Self {
        Self(ObserverLoadshareType::default())
    }

    /// Get the loadshare ID if there is one. Will return `None` if the loadshare type is auto
    /// and hasn't been granted a loadshare ID yet.
    pub fn get(&self) -> Option<LoadshareId> {
        match self.0 {
            ObserverLoadshareType::Auto => None,
            ObserverLoadshareType::Observer(id) => Some(id),
            ObserverLoadshareType::Manual(id) => Some(id),
        }
    }
}

/// A loadshare is a group of engine resources (permits, chunks, etc.) managed by some owner.
/// This owner is often an observer but doesn't have to be. When a chunk is loaded it's loaded under
/// a loadshare with some given load reasons. More chunks can be loaded under the same loadshare later.
/// When a chunk is unloaded, its load reasons are removed for a given loadshare. Chunks will be removed
/// from the loadshare if they have no remaining reasons. If a chunk isn't present in any loadshare, it can be
/// unloaded from the engine entirely. This same logic applies to permits.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct LoadshareId(u32);

impl nohash::IsEnabled for LoadshareId {}

pub type LoadshareMap<T> = hb::HashMap<LoadshareId, T, nohash::BuildNoHashHasher<LoadshareId>>;

/// Provides unique loadshare IDs. You can leak resources by getting a unique loadshare ID from the
/// provider, allocating a resource under that loadshare, and forgetting or losing track of the ID.
/// In this case there's no way to refer to the resources under that loadshare because you don't have
/// the ID anymore. For this reason always be careful to store your loadshare ID somewhere.
#[derive(Resource, Default)]
pub struct LoadshareProvider {
    loadshares: HashSet<LoadshareId, nohash::BuildNoHashHasher<LoadshareId>>,
}

const MAX_PROVIDER_RETRIES: usize = 16;

impl LoadshareProvider {
    /// Create a unique loadshare ID and store it internally so that it won't be provided again.
    pub fn create_loadshare(&mut self) -> LoadshareId {
        let mut id = LoadshareId(0);
        let mut retry = 0;

        while self.loadshares.contains(&id) {
            if retry >= MAX_PROVIDER_RETRIES {
                panic!("Couldn't create a unique loadshare within {MAX_PROVIDER_RETRIES} tries");
            }

            retry += 1;
            id = LoadshareId(rand::random::<u32>());
        }

        self.loadshares.insert(id);
        id
    }

    /// Check if this loadshare provider contains the given loadshare ID
    pub fn contains(&self, id: LoadshareId) -> bool {
        self.loadshares.contains(&id)
    }

    /// Remove a loadshare ID
    pub(crate) fn remove_loadshare(&mut self, id: LoadshareId) {
        self.loadshares.remove(&id);
    }
}

#[derive(
    Component,
    Clone,
    Copy,
    Debug,
    Deref,
    DerefMut,
    dm::Constructor,
    Hash,
    PartialEq,
    Eq,
    ExtractComponent,
)]
pub struct ObserverId(u32);

#[derive(Bundle)]
pub struct ObserverBundle {
    pub settings: ObserverSettings,
    pub chunks: RenderableObserverChunks,
    pub loadshare: ObserverLoadshare,
    pub id: ObserverId,
}

impl ObserverBundle {
    pub fn new(id: ObserverId) -> Self {
        Self {
            settings: Default::default(),
            chunks: Default::default(),
            loadshare: Default::default(),
            id,
        }
    }
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
    #[derive(Copy, Clone, Eq, PartialEq, Hash)]
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

impl fmt::Debug for LoadReasons {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let permit_flag_names = [
            (Self::MANUAL, "MANUAL"),
            (Self::RENDER, "RENDER"),
            (Self::COLLISION, "COLLISION"),
        ];

        let mut list = f.debug_list();

        for (flag, name) in permit_flag_names {
            if self.contains(flag) {
                list.entry(&name);
            }
        }

        list.finish()
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
            .init_resource::<LoadshareProvider>()
            .add_plugins(ExtractComponentPlugin::<ObserverId>::default())
            .add_event::<LoadChunksEvent>()
            .add_event::<LoadedChunkEvent>()
            .add_event::<UnloadChunksEvent>()
            .add_event::<UnloadedChunkEvent>()
            .add_event::<AddPermitFlagsEvent>()
            .add_event::<RemovePermitFlagsEvent>()
            .add_event::<PermitLostFlagsEvent>()
            .add_event::<ChunkObserverMoveEvent>()
            .add_event::<ChunkObserverCrossChunkBorderEvent>();

        app.add_systems(
            FixedPostUpdate,
            (
                dispatch_move_events.in_set(WorldControllerSystems::ObserverMovement),
                (
                    grant_observer_loadshares,
                    unload_out_of_range_chunks,
                    load_in_range_chunks,
                )
                    .chain()
                    .in_set(WorldControllerSystems::ObserverResponses),
                (
                    handle_chunk_loads_and_unloads,
                    handle_permit_flag_additions,
                    handle_permit_flag_removals,
                )
                    .chain()
                    .in_set(WorldControllerSystems::CoreEvents),
                generate_chunks_with_priority.after(WorldControllerSystems::CoreEvents),
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
