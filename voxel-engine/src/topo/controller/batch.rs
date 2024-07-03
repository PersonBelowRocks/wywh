use std::fmt;

use bevy::{
    ecs::{
        component::{ComponentHooks, StorageType},
        entity::{EntityHashMap, EntityHashSet},
        system::SystemId,
    },
    prelude::*,
    render::extract_component::ExtractComponent,
};
use bitflags::bitflags;
use hb::hash_map::Entry;

use crate::{
    render::lod::LevelOfDetail,
    topo::{controller::LoadshareId, world::ChunkPos},
    util::{ChunkMap, ChunkSet},
};

use super::{AddBatchChunks, RemoveBatchChunks, VoxelWorldTick};

bitflags! {
    #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    pub struct BatchFlags: u16 {
        const RENDER = 1 << 0;
        const COLLISION = 1 << 1;
    }
}

impl fmt::Debug for BatchFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let flag_names = [(Self::RENDER, "RENDER"), (Self::COLLISION, "COLLISION")];

        let mut list = f.debug_list();

        for (flag, name) in flag_names {
            if self.contains(flag) {
                list.entry(&name);
            }
        }

        list.finish()
    }
}

#[derive(Resource, Default)]
pub struct CachedBatchMembership(ChunkMap<BatchedChunk>);

struct BatchedChunk {
    in_batches: EntityHashSet,
    cached_flags: BatchFlags,
}

impl BatchedChunk {
    pub fn new(batches: &[Entity], flags: BatchFlags) -> Self {
        Self {
            in_batches: EntityHashSet::from_iter(batches.iter().cloned()),
            cached_flags: flags,
        }
    }
}

impl CachedBatchMembership {
    /// Cache the membership of the given chunk in the given batch. This will clear the cached flags
    /// and they must be rebuilt manually!
    pub(super) fn add(&mut self, chunk: ChunkPos, batch: Entity) {
        self.0
            .entry(chunk)
            .and_modify(|batched| {
                batched.in_batches.insert(batch);
                batched.cached_flags = BatchFlags::empty();
            })
            .or_insert_with(|| BatchedChunk::new(&[batch], BatchFlags::empty()));
    }

    /// Remove the given chunk from the given batch. This will clear the cached flags
    /// and they must be rebuilt manually!
    pub(super) fn remove(&mut self, chunk: ChunkPos, batch: Entity) {
        match self.0.entry(chunk) {
            Entry::Occupied(mut entry) => {
                let batched = entry.get_mut();
                batched.in_batches.remove(&batch);
                batched.cached_flags = BatchFlags::empty();

                if batched.in_batches.is_empty() {
                    entry.remove_entry();
                }
            }
            Entry::Vacant(_) => (),
        }
    }

    pub fn get(&self, chunk: ChunkPos) -> Option<&EntityHashSet> {
        self.0.get(chunk).map(|batched| &batched.in_batches)
    }

    pub fn has_flags(&self, chunk: ChunkPos, flags: BatchFlags) -> bool {
        self.0
            .get(chunk)
            .is_some_and(|batched| batched.cached_flags.contains(flags))
    }

    fn set_cached_flags(&mut self, chunk: ChunkPos, flags: BatchFlags) {
        self.0.get_mut(chunk).map(|b| b.cached_flags = flags);
    }
}

#[derive(Event, Debug)]
pub struct UpdateCachedChunkFlags(pub ChunkSet);

pub fn update_cached_chunk_flags(
    trigger: Trigger<UpdateCachedChunkFlags>,
    q_batches: Query<&ChunkBatch>,
    mut membership: ResMut<CachedBatchMembership>,
) {
    let event = trigger.event();

    for chunk in event.0.iter() {
        let Some(batch_entities) = membership.get(chunk) else {
            continue;
        };

        let mut flags = BatchFlags::empty();
        for &batch_entity in batch_entities.iter() {
            let batch = q_batches.get(batch_entity).unwrap();
            flags |= batch.flags;
        }

        membership.set_cached_flags(chunk, flags);
    }
}

/// A batch of chunks
#[derive(Clone, Component)]
pub struct ChunkBatch {
    owner: Entity,
    flags: BatchFlags,
    chunks: ChunkSet,
    tick: u64,
}

#[derive(Copy, Clone, Component, PartialEq, Eq, Deref, DerefMut)]
pub struct ChunkBatchLod(pub LevelOfDetail);

impl ChunkBatch {
    pub fn new(owner: Entity, flags: BatchFlags) -> Self {
        Self {
            owner,
            flags,
            chunks: Default::default(),
            tick: 0,
        }
    }

    pub fn manually_register_hooks(hooks: &mut ComponentHooks) {
        // Add this batch entity to its owner when the component is added
        hooks.on_insert(|mut world, batch_entity, _id| {
        let tick = world.resource::<VoxelWorldTick>().get();

        // Set the tick to the current tick upon insertion. The default value in this field is
        // just a placeholder and must be replaced!
        let mut this = world.get_mut::<Self>(batch_entity).unwrap();
        this.tick = tick;

        let owner = this.owner;

        let Some(mut observer_batches) = world.get_mut::<ObserverBatches>(owner) else {
            error!("Chunk batch for entity {batch_entity:?} wants to be owned by {owner:?}, 
                but that entity either doesn't exist or doesn't have an 'ObserverBatches' component.");
            panic!();
        };

        observer_batches.owned.insert(batch_entity);
    });

        // Remove this batch entity from its owner when the component is removed
        hooks.on_remove(|mut world, batch_entity, _id| {
            let owner = world.get::<Self>(batch_entity).unwrap().owner;

            let Some(mut observer_batches) = world.get_mut::<ObserverBatches>(owner) else {
                // This batch is being removed, so if it doesn't have a valid owner entity it's not a big deal
                return;
            };

            observer_batches.owned.remove(&batch_entity);
        });
    }

    pub fn num_chunks(&self) -> u32 {
        self.chunks.len() as _
    }

    pub fn chunks(&self) -> &ChunkSet {
        &self.chunks
    }

    pub fn flags(&self) -> BatchFlags {
        self.flags
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn owner(&self) -> Entity {
        self.owner
    }
}

pub fn add_batch_chunks(
    trigger: Trigger<AddBatchChunks>,
    tick: Res<VoxelWorldTick>,
    mut q_batches: Query<&mut ChunkBatch>,
    mut membership: ResMut<CachedBatchMembership>,
    mut cmds: Commands,
) {
    let batch_entity = trigger.entity();
    let event = trigger.event();

    if event.0.is_empty() {
        return;
    }

    let mut batch = match q_batches.get_mut(batch_entity) {
        Ok(batch) => batch,
        Err(error) => {
            warn!("Could not run trigger `AddBatchChunks` for {batch_entity}: {error}");
            return;
        }
    };

    // Set the update tick to the current tick
    batch.tick = tick.get();
    batch.chunks.extend(
        event
            .0
            .iter()
            .inspect(|&chunk| membership.add(chunk, batch_entity)),
    );

    // Rebuild the cached flags for the added chunks
    cmds.trigger(UpdateCachedChunkFlags(event.0.clone()));
}

pub fn remove_batch_chunks(
    trigger: Trigger<RemoveBatchChunks>,
    tick: Res<VoxelWorldTick>,
    mut q_batches: Query<&mut ChunkBatch>,
    mut membership: ResMut<CachedBatchMembership>,
    mut cmds: Commands,
) {
    let batch_entity = trigger.entity();
    let event = trigger.event();

    if event.0.is_empty() {
        return;
    }

    let mut batch = match q_batches.get_mut(batch_entity) {
        Ok(batch) => batch,
        Err(error) => {
            warn!("Could not run trigger `RemoveBatchChunks` for {batch_entity}: {error}");
            return;
        }
    };

    // Set the update tick to the current tick
    batch.tick = tick.get();
    for chunk in event.0.iter() {
        batch.chunks.remove(chunk);
        membership.remove(chunk, batch_entity);
    }

    // Rebuild the cached flags for the removed chunks
    cmds.trigger(UpdateCachedChunkFlags(event.0.clone()));
}

/// The batches that an observer can render and update
#[derive(Default, Clone, Component)]
pub struct ObserverBatches {
    /// The batches this observer owns. Should never be manually updated, rather you should spawn batches and
    /// specify this entity as their owner. The engine will automatically update the owner's batches accordingly.
    pub owned: EntityHashSet,
}

// TODO: trigger for adding this component to an observer
#[derive(Component, Clone, Debug, Deref, DerefMut, dm::Constructor, Default)]
pub struct VisibleBatches(pub EntityHashSet);
