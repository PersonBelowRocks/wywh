use bevy::{
    ecs::{
        component::{ComponentHook, ComponentHooks, Immutable, StorageType},
        entity::EntityHashMap,
        system::SystemParam,
    },
    prelude::*,
};
use either::Either;
use hb::hash_map::Entry;

use crate::util::ChunkMap;

use super::ChunkPos;

/// Type alias for an [`Either`] enum with the chunk position on the left, and the entity on the right.
/// Used to operations on the [`ChunkEntityLink`] that can use either form of chunk identification (like [`ChunkEntityLink::remove`]).
pub type ChunkPosOrEntity = Either<ChunkPos, Entity>;

/// Links chunks (more specifically their chunk positions) to an entity in the ECS world.
/// Allows for an [`Entity`] to be obtained from a [`ChunkPos`], and vice versa.
///
/// Used to find which entity to query based on a chunk position, essentially allowing chunks to be queried using their position.
#[derive(Resource, Default)]
pub struct ChunkEntityLink {
    chunk_positions: EntityHashMap<ChunkPos>,
    entities: ChunkMap<Entity>,
}

impl ChunkEntityLink {
    /// Get the chunk position for the given chunk entity. If the entity was not a chunk entity, or didn't exist, return [`None`].
    #[inline]
    #[must_use]
    pub fn chunk_pos(&self, entity: Entity) -> Option<ChunkPos> {
        self.chunk_positions.get(&entity).copied()
    }

    /// Get the entity for the given chunk position. If there's no chunk at this position return [`None`].
    #[inline]
    #[must_use]
    pub fn entity(&self, chunk_pos: ChunkPos) -> Option<Entity> {
        self.entities.get(chunk_pos).copied()
    }

    /// Insert a new link between the given chunk position and entity, overwriting any existing entries involving them.
    #[inline]
    fn insert_link(&mut self, chunk_pos: ChunkPos, entity: Entity) {
        match self.entities.entry(chunk_pos) {
            Entry::Occupied(entry) => {
                let old_entity = entry.get();
                self.chunk_positions
                    .remove(old_entity)
                    .expect("this map is bidirectional");
            }
            Entry::Vacant(entry) => {
                entry.insert(entity);
            }
        }

        match self.chunk_positions.entry(entity) {
            Entry::Occupied(entry) => {
                let old_chunk_position = entry.get();
                self.entities
                    .remove(*old_chunk_position)
                    .expect("this map is bidirectional");
            }
            Entry::Vacant(entry) => {
                entry.insert(chunk_pos);
            }
        }
    }

    /// Remove a link entirely, erasing both the entity and chunk position from this datastructure.
    /// Can use any form of chunk identification to perform the removal.
    ///
    /// Returns the other half of the provided link. For example:
    /// - If remove is called with a [`ChunkPos`], an [`Either::Right`] is returned with the removed [`Entity`]
    /// - If remove is called with an [`Entity`], an [`Either::Left`] is returned with the removed [`ChunkPos`]
    ///
    /// If no link existed for the given chunk, returns [`None`]
    #[inline]
    fn remove_link(&mut self, either: ChunkPosOrEntity) -> Option<ChunkPosOrEntity> {
        match either {
            Either::Left(chunk_pos) => {
                let entity = self.entities.remove(chunk_pos)?;
                self.chunk_positions
                    .remove(&entity)
                    .expect("this map is bidirectional");
                Some(Either::Right(entity))
            }
            Either::Right(entity) => {
                let chunk_pos = self.chunk_positions.remove(&entity)?;
                self.entities
                    .remove(chunk_pos)
                    .expect("this map is bidirectional");
                Some(Either::Left(chunk_pos))
            }
        }
    }
}

impl Component for ChunkPos {
    const STORAGE_TYPE: StorageType = StorageType::Table;
    // The chunk position is immutable since we don't want chunks moving around after they've been put in place.
    type Mutability = Immutable;

    fn register_component_hooks(hooks: &mut ComponentHooks) {
        // link the chunk position and entity on insertion
        hooks.on_insert(|mut world, context| {
            let chunk_pos = *world.get::<Self>(context.entity).unwrap();
            let mut link = world.resource_mut::<ChunkEntityLink>();

            link.insert_link(chunk_pos, context.entity);
        });

        // remove the link on removal
        hooks.on_remove(|mut world, context| {
            let mut link = world.resource_mut::<ChunkEntityLink>();

            let removed_chunk_pos = link
                .remove_link(Either::Right(context.entity))
                .unwrap()
                .unwrap_left();

            // optional sanity checking to ensure linked chunk position matched the chunk position on the entity
            #[cfg(debug_assertions)]
            {
                let chunk_pos = *world.get::<Self>(context.entity).unwrap();
                debug_assert_eq!(
                    chunk_pos, removed_chunk_pos,
                    "removed chunk position did not match entity's linked chunk position"
                )
            }
        });
    }
}
