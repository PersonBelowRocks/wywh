use bevy::ecs::{
    entity::{Entity, EntityHashMap},
    system::Resource,
};
use bitflags::bitflags;

use crate::{topo::world::ChunkPos, util::ChunkMap};

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
    pub struct PermitFlags: u16 {
        const RENDER = 1 << 0;
        const COLLISION = 1 << 1;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Permit {
    pub flags: PermitFlags,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub entity: Entity,
    pub chunk: ChunkPos,
    pub permit: Permit,
}

#[derive(Copy, Clone, Debug)]
pub enum ChunkPermitKey {
    Chunk(ChunkPos),
    Entity(Entity),
}

#[derive(Copy, Clone, Debug)]
pub enum ChunkPermitKeys<'a> {
    Chunk(&'a [ChunkPos]),
    Entity(&'a [Entity]),
}

#[derive(Resource, Default)]
pub struct ChunkPermits {
    entity_keys: EntityHashMap<usize>,
    chunk_keys: ChunkMap<usize>,
    data: Vec<Entry>,
}

impl ChunkPermits {
    fn get_idx(&self, key: ChunkPermitKey) -> Option<usize> {
        Some(*match key {
            ChunkPermitKey::Chunk(cpos) => self.chunk_keys.get(cpos)?,
            ChunkPermitKey::Entity(entity) => self.entity_keys.get(&entity)?,
        })
    }

    pub fn insert(&mut self, entity: Entity, chunk: ChunkPos, permit: Permit) {
        let entry = Entry {
            entity,
            chunk,
            permit,
        };

        let idx = self.data.len();
        self.data.push(entry);

        self.entity_keys.insert(entity, idx);
        self.chunk_keys.set(chunk, idx);
    }

    pub fn remove(&mut self, key: ChunkPermitKey) -> Option<Entry> {
        let idx = self.get_idx(key)?;

        let removed_entry = self.data.swap_remove(idx);

        let swapped_entry = &self.data[idx];
        self.chunk_keys.set(swapped_entry.chunk, idx);
        self.entity_keys.insert(swapped_entry.entity, idx);

        Some(removed_entry)
    }

    pub fn get(&self, key: ChunkPermitKey) -> Option<&Permit> {
        let idx = self.get_idx(key)?;
        Some(&self.data[idx].permit)
    }

    pub fn get_mut(&mut self, key: ChunkPermitKey) -> Option<&mut Permit> {
        let idx = self.get_idx(key)?;
        Some(&mut self.data[idx].permit)
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}
