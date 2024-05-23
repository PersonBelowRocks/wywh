use bevy::ecs::entity::{Entity, EntityHash};
use fxhash::FxBuildHasher;

use super::ChunkPos;

type InnerType = bimap::BiHashMap<Entity, ChunkPos, EntityHash, FxBuildHasher>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pair {
    pub entity: Entity,
    pub chunk: ChunkPos,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum CEBimapValue {
    Chunk(ChunkPos),
    Entity(Entity),
}

#[derive(Clone, PartialEq, Eq)]
pub struct CEBimap(InnerType);

impl CEBimap {
    pub fn new() -> Self {
        Self(InnerType::default())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn insert(&mut self, pair: Pair) {
        self.0.insert(pair.entity, pair.chunk);
    }

    pub fn remove(&mut self, value: CEBimapValue) -> Option<Pair> {
        match value {
            CEBimapValue::Chunk(cpos) => self.0.remove_by_right(&cpos),
            CEBimapValue::Entity(entity) => self.0.remove_by_left(&entity),
        }
        .map(|(entity, chunk)| Pair { entity, chunk })
    }

    pub fn get_chunk(&self, entity: Entity) -> Option<ChunkPos> {
        self.0.get_by_left(&entity).copied()
    }

    pub fn get_entity(&self, chunk: ChunkPos) -> Option<Entity> {
        self.0.get_by_right(&chunk).copied()
    }

    pub fn pairs(&self) -> impl Iterator<Item = Pair> + '_ {
        self.0
            .iter()
            .map(|(&entity, &chunk)| Pair { entity, chunk })
    }

    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit()
    }
}
