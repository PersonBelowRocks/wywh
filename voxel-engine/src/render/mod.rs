pub mod core;
mod lod;
pub mod mesh;
pub mod meshing;
pub mod quad;

use bevy::{
    ecs::{
        component::{ComponentHooks, StorageType},
        entity::EntityHashSet,
        system::SystemId,
    },
    prelude::*,
    render::extract_component::ExtractComponent,
};
pub use lod::*;

use crate::util::ChunkSet;

/// A batch of chunks that can be rendered
// TODO: remove orphaned chunk batches
#[derive(Clone, ExtractComponent)]
pub struct ChunkBatch {
    /// The observer that owns this batch. If this is `None` then this batch is orphaned.
    pub owner: Option<Entity>,
    pub lod: LevelOfDetail,
    pub chunks: ChunkSet,
    pub tick: u64,
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

            // Do nothing if this batch is an orphan
            let Some(owner) = owner else {
                return
            };

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

            // Do nothing if this batch is an orphan
            let Some(owner) = owner else {
                return
            };

            let Some(mut observer_batches) = world.get_mut::<ObserverBatches>(owner) else {
                error!("Chunk batch for entity {batch_entity:?} wants to be owned by {owner:?}, 
                    but that entity either doesn't exist or doesn't have an 'ObserverBatches' component.");
                panic!();
            };

            observer_batches.owned.remove(&batch_entity);
        });
    }
}

/// The batches that an observer can render and update
#[derive(Clone)]
pub struct ObserverBatches {
    /// The batches this observer owns. Should never be manually updated, rather you should spawn batches and
    /// specify this entity as their owner. The engine will automatically update the owner's batches accordingly.
    pub owned: EntityHashSet,
}

impl Component for ObserverBatches {
    const STORAGE_TYPE: StorageType = StorageType::Table;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        hooks.on_remove(|mut world, observer_entity, _id| {
            let batches = world.get::<Self>(observer_entity).unwrap().owned.clone();

            for &entity in batches.iter() {
                let Some(mut batch) = world.get_mut::<ChunkBatch>(entity) else {
                    continue;
                };

                batch.owner = None;
            }
        });
    }
}

#[derive(Component, Clone, Debug, Deref, DerefMut, dm::Constructor, Default)]
pub struct VisibleBatches(EntityHashSet);
