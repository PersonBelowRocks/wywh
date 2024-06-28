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
pub struct BatchMembership(ChunkMap<BatchedChunk>);

struct BatchedChunk {
    in_batches: EntityHashSet,
    cached_flags: Option<BatchFlags>,
}

impl BatchedChunk {
    pub fn new(batches: &[Entity]) -> Self {
        Self {
            in_batches: EntityHashSet::from_iter(batches.iter().cloned()),
            cached_flags: None,
        }
    }
}

impl BatchMembership {
    pub fn add(&mut self, chunk: ChunkPos, batch: Entity) {
        self.0
            .entry(chunk)
            .and_modify(|batched| {
                batched.in_batches.insert(batch);
                batched.cached_flags = None;
            })
            .or_insert_with(|| BatchedChunk::new(&[batch]));
    }

    pub fn remove(&mut self, chunk: ChunkPos, batch: Entity) {
        match self.0.entry(chunk) {
            Entry::Occupied(mut entry) => {
                let batched = entry.get_mut();
                batched.in_batches.remove(&batch);
                batched.cached_flags = None;

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

    fn set_cached_flags(&mut self, chunk: ChunkPos, flags: BatchFlags) {
        self.0.get_mut(chunk).map(|b| b.cached_flags = Some(flags));
    }
}

#[derive(Event, Debug)]
pub struct UpdateCachedChunkFlags(pub ChunkSet);

pub fn update_cached_chunk_flags(
    trigger: Trigger<UpdateCachedChunkFlags>,
    q_batches: Query<&ChunkBatch>,
    mut membership: ResMut<BatchMembership>,
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
#[derive(Clone, ExtractComponent)]
pub struct ChunkBatch {
    pub owner: Entity,
    pub flags: BatchFlags,
    pub chunks: ChunkSet,
    pub tick: u64,

    // TODO: should be its own component tbh
    pub lod: LevelOfDetail,
}

impl ChunkBatch {
    pub fn num_chunks(&self) -> u32 {
        self.chunks.len() as _
    }
}

impl Component for ChunkBatch {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        // Add this batch entity to its owner when the component is added
        hooks.on_insert(|mut world, batch_entity, _id| {
            let owner = world.get::<Self>(batch_entity).unwrap().owner;

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
}

/// The batches that an observer can render and update
#[derive(Clone, Component)]
pub struct ObserverBatches {
    /// The batches this observer owns. Should never be manually updated, rather you should spawn batches and
    /// specify this entity as their owner. The engine will automatically update the owner's batches accordingly.
    pub owned: EntityHashSet,
}

#[derive(Component, Clone, Debug, Deref, DerefMut, dm::Constructor, Default)]
pub struct VisibleBatches(EntityHashSet);
