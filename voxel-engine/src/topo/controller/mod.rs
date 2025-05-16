use std::sync::Arc;
use std::time::Duration;

use async_bevy_events::{AsyncEventPlugin, EventFunnelPlugin};
use bevy::ecs::component::{ComponentHooks, HookContext, Mutable, StorageType};
use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::world::DeferredWorld;
use bevy::math::ivec3;
use bevy::prelude::*;
use bevy::time::Stopwatch;

use actor_events::{
    populate_loaded_chunks, send_priority_recalculation_events, trigger_actor_chunk_border_events,
};

use crate::data::registries::block::BlockVariantRegistry;
use crate::data::registries::{REGISTRY_MANAGER, Registry};
use crate::data::resourcepath::rpath;
use crate::topo::world::chunk_manager::ecs::{
    start_async_chunk_load_task, start_async_chunk_purge_task,
};
use crate::topo::world::chunk_populator::ChunkPopulatorController;
use crate::util::ChunkSet;
use crate::util::sync::LockStrategy;
use crate::{CoreEngineSetup, EngineState};

use super::world::chunk_manager::ecs::{ChunkLifecycleTaskLockGranularity, ChunkManagerRes};
use super::world::{ChunkManager, ChunkPos, VoxelRealm};

mod actor_events;
mod error;
mod events;
pub use events::*;

use crate::topo::world::chunk_manager::ChunkNotification;
use octo::Region;

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

/// The region (in chunkspace) around a chunk actor which will be loaded automatically.
/// `[0, 0, 0]` in the region corresponds to the chunk the actor is currently in.
#[derive(Clone, Component, Debug, dm::Into, dm::Deref, dm::DerefMut)]
pub struct ActorLoadRegion(pub Region);

impl Default for ActorLoadRegion {
    fn default() -> Self {
        const MIN: IVec3 = ivec3(-4, -2, -4);
        const MAX: IVec3 = ivec3(4, 2, 4);
        Self(Region::new_inclusive(MIN, MAX))
    }
}

/// A component attached to chunks, listing all the actor entities that have this chunk loaded.
#[derive(Clone, Component, Debug, dm::Deref, dm::DerefMut, Default)]
pub struct ChunkActors(EntityHashSet);

/// A component attached to an actor, listing all the chunk entities the actor has loaded.
#[derive(Clone, Component, Debug, dm::Deref, dm::DerefMut, Default)]
pub struct ActorLoadedChunks(EntityHashSet);

#[derive(Bundle, Default)]
pub struct ObserverBundle {
    pub settings: ActorLoadRegion,
}

impl ObserverBundle {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The previous position of an actor. Used for detecting when an actor crosses a chunk border.
/// When adding this component to an entity, a [`CrossChunkBorder`] event is triggered on that entity.
#[derive(Clone, Component, Debug)]
#[component(on_add = PreviousActorPosition::on_add)]
pub struct PreviousActorPosition {
    pub ws_pos: Vec3,
    pub chunk_pos: ChunkPos,
}

impl PreviousActorPosition {
    /// `on_insert` hook, will trigger a [`CrossChunkBorder`] event on the entity it's inserted on if this was the first time it was inserted.
    #[inline]
    pub fn on_add(mut world: DeferredWorld, context: HookContext) {
        let this = world.get::<Self>(context.entity).unwrap();

        world.trigger_targets(
            CrossChunkBorder {
                new: true,
                old_chunk: this.chunk_pos,
                new_chunk: this.chunk_pos,
            },
            context.entity,
        );
    }
}

/// System sets for the world controller
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, SystemSet)]
pub enum WorldControllerSystems {
    CoreEvents,
    ActorMovement,
    ActorResponses,
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
            ChunkPopulatorController,
            AsyncEventPlugin::<LoadChunksEvent>::default(),
            AsyncEventPlugin::<UnloadChunksEvent>::default(),
            EventFunnelPlugin::<LoadedChunkEvent>::for_new(),
            EventFunnelPlugin::<PurgedChunkEvent>::for_new(),
        ))
        .insert_resource(self.settings)
        .init_resource::<VoxelWorldTick>()
        .insert_resource(DEFAULT_LOCK_GRANULARITY);

        app.add_systems(
            OnEnter(EngineState::Finished),
            initialize_chunk_manager.in_set(CoreEngineSetup::InitializeChunkManager),
        );

        app.add_systems(
            OnEnter(EngineState::Finished),
            (start_async_chunk_load_task, start_async_chunk_purge_task)
                .chain()
                .in_set(CoreEngineSetup::Initialize),
        );

        app.add_systems(
            PostUpdate,
            (
                trigger_actor_chunk_border_events.in_set(WorldControllerSystems::ActorMovement),
                (send_priority_recalculation_events, populate_loaded_chunks)
                    .in_set(WorldControllerSystems::ActorResponses),
                (forward_chunk_notifications, unload_purged_chunks)
                    .in_set(WorldControllerSystems::CoreEvents),
            ),
        );

        app.add_systems(FixedLast, increase_voxel_world_tick);

        app.configure_sets(
            PostUpdate,
            (
                WorldControllerSystems::ActorMovement,
                WorldControllerSystems::ActorResponses,
                WorldControllerSystems::CoreEvents,
            )
                .chain()
                .run_if(in_state(EngineState::Finished)),
        );
    }
}

/// System for forwarding chunk notifications to bevy events that can be used by other systems.
pub fn forward_chunk_notifications(
    realm: VoxelRealm,
    mut events: EventWriter<ChunkNotification>,
    // for batching the notifications so we allocate memory slightly more efficiently
    mut last_num_notifications: Local<usize>,
) {
    let bus = realm.cm().notification_bus();
    let mut notifications = Vec::with_capacity(*last_num_notifications);

    while let Ok(notification) = bus.receiver().try_recv() {
        notifications.push(notification);
    }

    *last_num_notifications = notifications.len();
    events.write_batch(notifications);
}

/// System for initializing the chunk manager and adding it as a resource.
/// Must be run after registries have been built.
pub fn initialize_chunk_manager(world: &mut World) {
    let chunk_manager = {
        let varreg = REGISTRY_MANAGER
            .get_registry::<BlockVariantRegistry>()
            .unwrap();
        let void = varreg
            .get_id(&rpath(BlockVariantRegistry::RPATH_VOID))
            .unwrap();

        ChunkManager::new(void)
    };

    world.insert_resource(ChunkManagerRes(Arc::new(chunk_manager)));
}

pub const UNLOAD_BACKLOG_INTERVAL: Duration = Duration::from_millis(10);

/// System for unloading purged chunks. Chunks exist in purgatory for a little before they are
/// unloaded and their resources freed. Chunks are only unloaded when they have no more references to them.
///
/// This system is only really temporary until a more proper chunk unloading solution is implemented.
/// Chunks are not saved to disk or anything in this system, they are just immediatelly dropped as soon as possible.
pub fn unload_purged_chunks(
    time: Res<Time<Real>>,
    mut purged_events: EventReader<PurgedChunkEvent>,
    mut backlog: Local<ChunkSet>,
    mut time_since_last_backlog_unload: Local<Stopwatch>,
    realm: VoxelRealm,
) {
    time_since_last_backlog_unload.tick(time.delta());

    // Only unload if there are pending events or if we've waited long enough for our backlog.
    if purged_events.is_empty()
        || time_since_last_backlog_unload.elapsed() < UNLOAD_BACKLOG_INTERVAL
    {
        return;
    }

    // TODO: better locking policy
    realm
        .cm()
        .structural_access(LockStrategy::Blocking, |access| {
            backlog.retain(|&chunk_pos| {
                let Some((_, arc_chunk)) = access.purgatory.remove(&chunk_pos) else {
                    return false;
                };

                let removed_chunk = match Arc::try_unwrap(arc_chunk) {
                    Ok(chunk) => chunk,
                    Err(arc_chunk) => {
                        access.purgatory.insert(chunk_pos, arc_chunk);

                        return true;
                    }
                };

                drop(removed_chunk);
                false
            });

            for event in purged_events.read() {
                let Some((_, arc_chunk)) = access.purgatory.remove(&event.chunk_pos) else {
                    continue;
                };

                let removed_chunk = match Arc::try_unwrap(arc_chunk) {
                    Ok(chunk) => chunk,
                    Err(arc_chunk) => {
                        access.purgatory.insert(event.chunk_pos, arc_chunk);
                        backlog.set(event.chunk_pos);

                        continue;
                    }
                };

                drop(removed_chunk);
            }

            time_since_last_backlog_unload.reset();
        })
        .unwrap();
}
