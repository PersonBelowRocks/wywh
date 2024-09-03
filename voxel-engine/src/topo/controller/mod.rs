use std::sync::Arc;
use std::{fmt, time::Duration};

use async_bevy_events::{AsyncEventPlugin, EventFunnelPlugin};
use bevy::ecs::component::{ComponentHooks, StorageType};
use bevy::math::ivec3;
use bevy::prelude::*;
use bitflags::bitflags;
use hb::HashSet;

use observer_events::{
    dispatch_move_events, generate_chunks_with_priority, update_observer_batches,
};

use crate::data::registries::block::BlockVariantRegistry;
use crate::data::registries::{Registries, Registry};
use crate::data::resourcepath::rpath;
use crate::topo::world::chunk_manager::ecs::{
    start_async_chunk_load_task, start_async_chunk_purge_task,
};
use crate::topo::worldgen::ecs::setup_terrain_generator_workers;
use crate::topo::worldgen::generator::GenerateChunk;
use crate::{CoreEngineSetup, EngineState};

use super::bounding_box::BoundingBox;
use super::world::chunk_manager::ecs::{ChunkLifecycleTaskLockGranularity, ChunkManagerRes};
use super::world::{ChunkManager, ChunkPos};

mod error;
mod events;
mod handle_events;
mod observer_events;
pub use events::*;

mod batch;
pub use batch::*;

#[derive(Resource, Default)]
pub struct VoxelWorldTick(u64);

impl VoxelWorldTick {
    pub fn get(&self) -> u64 {
        self.0
    }
}

fn increase_voxel_world_tick(mut tick: ResMut<VoxelWorldTick>) {
    tick.0 += 1;
}

// TODO: use ints not floats!
#[derive(Clone, Component, Debug)]
pub struct ObserverSettings {
    pub horizontal_range: u32,
    pub view_distance_above: u32,
    pub view_distance_below: u32,
}

impl Default for ObserverSettings {
    fn default() -> Self {
        Self {
            horizontal_range: 4,
            view_distance_above: 2,
            view_distance_below: 2,
        }
    }
}

impl ObserverSettings {
    pub fn within_range(&self, opos: ChunkPos, cpos: ChunkPos) -> bool {
        let diff = cpos.as_ivec3() - opos.as_ivec3();

        let max_hor_diff = i32::max(diff.x.abs(), diff.z.abs()) as u32;
        // Positive if chunk is above the observer, negative if it's below
        let height_diff = diff.y;

        max_hor_diff <= self.horizontal_range
            && height_diff <= (self.view_distance_above as i32)
            && height_diff >= -(self.view_distance_below as i32)
    }

    pub fn bounding_box(&self) -> BoundingBox {
        let min = -ivec3(
            self.horizontal_range as i32,
            self.view_distance_below as i32,
            self.horizontal_range as i32,
        );
        let max = ivec3(
            self.horizontal_range as i32,
            self.view_distance_above as i32,
            self.horizontal_range as i32,
        );

        BoundingBox::new(min, max)
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
#[derive(Default, Copy, Clone, Debug, PartialEq, Eq)]
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

impl Component for ObserverLoadshare {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        // Automatically create a loadshare if this component is marked as auto
        hooks.on_insert(|mut world, entity, _id| {
            let loadshare_type = world.get_mut::<Self>(entity).unwrap().0;

            if loadshare_type == ObserverLoadshareType::Auto {
                let mut provider = world.resource_mut::<LoadshareProvider>();
                let loadshare_id = provider.create_loadshare();

                // Need to get the entity again here because we can't hold 2 mutable references to the world at the same time.
                world.get_mut::<Self>(entity).unwrap().0 =
                    ObserverLoadshareType::Observer(loadshare_id);
            }
        });

        // TODO: hook for removing an entire loadshare if this component is removed
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
pub type LoadshareIdHasher = nohash::BuildNoHashHasher<LoadshareId>;

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

#[derive(Bundle, Default)]
pub struct ObserverBundle {
    pub settings: ObserverSettings,
    pub batches: ObserverBatches,
    pub loadshare: ObserverLoadshare,
}

impl ObserverBundle {
    pub fn new() -> Self {
        Self::default()
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

pub const DEFAULT_LOCK_GRANULARITY: ChunkLifecycleTaskLockGranularity =
    ChunkLifecycleTaskLockGranularity(16);

pub struct WorldController {
    pub settings: WorldControllerSettings,
}

impl Plugin for WorldController {
    fn build(&self, app: &mut App) {
        info!("Initializing world controller");

        app.add_plugins((
            AsyncEventPlugin::<LoadChunks>::default(),
            AsyncEventPlugin::<UnloadChunks>::default(),
            EventFunnelPlugin::<LoadedChunkEvent>::for_new(),
            EventFunnelPlugin::<PurgedChunkEvent>::for_new(),
            EventFunnelPlugin::<LoadReasonsAddedEvent>::for_new(),
            EventFunnelPlugin::<LoadReasonsRemovedEvent>::for_new(),
        ))
        .add_event::<GenerateChunk>()
        .insert_resource(self.settings)
        .init_resource::<LoadshareProvider>()
        .init_resource::<VoxelWorldTick>()
        .init_resource::<CachedBatchMembership>()
        .insert_resource(DEFAULT_LOCK_GRANULARITY)
        .add_event::<AddBatchChunks>()
        .add_event::<RemoveBatchChunks>()
        .add_event::<RemovedBatchChunks>();

        // We need to manually register the hooks in this world (the main world) only. Otherwise the hooks will run in the render
        // world which 1. doesn't make any sense and 2. is impossible because the resources needed for the hooks to run
        // aren't there.
        let chunk_batch_hooks = app.world_mut().register_component_hooks::<ChunkBatch>();
        ChunkBatch::manually_register_hooks(chunk_batch_hooks);

        app.observe(update_observer_batches)
            .observe(add_batch_chunks)
            .observe(remove_batch_chunks);

        app.add_systems(
            OnEnter(EngineState::Finished),
            initialize_chunk_manager.in_set(CoreEngineSetup::InitializeChunkManager),
        );

        app.add_systems(
            OnEnter(EngineState::Finished),
            (
                start_async_chunk_load_task,
                start_async_chunk_purge_task,
                setup_terrain_generator_workers,
            )
                .chain()
                .in_set(CoreEngineSetup::Initialize),
        );

        app.add_systems(
            FixedPostUpdate,
            (
                dispatch_move_events.in_set(WorldControllerSystems::ObserverMovement),
                // handle_chunk_loads_and_unloads.in_set(WorldControllerSystems::CoreEvents),
                generate_chunks_with_priority.after(WorldControllerSystems::CoreEvents),
            ),
        );

        app.add_systems(FixedLast, increase_voxel_world_tick);

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

fn initialize_chunk_manager(world: &mut World) {
    let chunk_manager = {
        let registries = world.resource::<Registries>();
        let varreg = registries.get_registry::<BlockVariantRegistry>().unwrap();
        let void = varreg
            .get_id(&rpath(BlockVariantRegistry::RPATH_VOID))
            .unwrap();

        ChunkManager::new(void)
    };

    world.insert_resource(ChunkManagerRes(Arc::new(chunk_manager)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observer_ranges() {
        let settings = ObserverSettings {
            horizontal_range: 10,
            view_distance_above: 8,
            view_distance_below: 5,
        };

        for x in -10..=10 {
            for z in -10..=10 {
                for y in -5..=8 {
                    let cpos = ChunkPos::new(x, y, z);

                    assert!(settings.within_range(ChunkPos::ZERO, cpos))
                }
            }
        }

        assert!(!settings.within_range(ChunkPos::ZERO, ChunkPos::new(11, 5, -7)));
    }
}
