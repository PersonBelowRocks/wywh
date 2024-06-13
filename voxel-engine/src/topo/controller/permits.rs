use std::fmt;

use bevy::ecs::{
    entity::{Entity, EntityHashMap},
    system::Resource,
};
use bitflags::bitflags;

use crate::topo::controller::LoadshareMap;
use crate::{topo::world::ChunkPos, util::ChunkMap};

bitflags! {
    #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    pub struct PermitFlags: u16 {
        const RENDER = 1 << 0;
        const COLLISION = 1 << 1;
    }
}

impl fmt::Debug for PermitFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let permit_flag_names = [(Self::RENDER, "RENDER"), (Self::COLLISION, "COLLISION")];

        let mut list = f.debug_list();

        for (flag, name) in permit_flag_names {
            if self.contains(flag) {
                list.entry(&name);
            }
        }

        list.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Permit {
    pub loadshares: LoadshareMap<PermitFlags>,
    pub cached_flags: PermitFlags,
}

impl Permit {
    /// Updates the cached permit flags and returns them.
    /// Should be called whenever a loadshare updates permit flags.
    pub fn update_cached_flags(&mut self) -> PermitFlags {
        let mut cached = PermitFlags::empty();

        for &flags in self.loadshares.values() {
            cached |= flags;
        }

        self.cached_flags = cached;
        cached
    }
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
pub struct ChunkEcsPermits {
    entity_keys: EntityHashMap<usize>,
    chunk_keys: ChunkMap<usize>,
    data: Vec<Entry>,
}

impl ChunkEcsPermits {
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

        self.chunk_keys.remove(removed_entry.chunk);
        self.entity_keys.remove(&removed_entry.entity);

        if let Some(swapped_entry) = self.data.get(idx) {
            self.chunk_keys.set(swapped_entry.chunk, idx);
            self.entity_keys.insert(swapped_entry.entity, idx);
        }

        Some(removed_entry)
    }

    pub fn get_entity(&self, key: ChunkPos) -> Option<Entity> {
        let idx = self.get_idx(ChunkPermitKey::Chunk(key))?;
        Some(self.data[idx].entity)
    }

    pub fn get_chunk_pos(&self, key: Entity) -> Option<ChunkPos> {
        let idx = self.get_idx(ChunkPermitKey::Entity(key))?;
        Some(self.data[idx].chunk)
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

    pub fn iter(&self) -> ChunkEcsPermitsIterator<'_> {
        ChunkEcsPermitsIterator {
            current_idx: 0,
            permits: &self,
        }
    }
}

pub struct ChunkEcsPermitsIterator<'a> {
    permits: &'a ChunkEcsPermits,
    current_idx: usize,
}

impl<'a> Iterator for ChunkEcsPermitsIterator<'a> {
    type Item = &'a Entry;

    fn next(&mut self) -> Option<Self::Item> {
        self.current_idx += 1;
        self.permits.data.get(self.current_idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_ecs_permits() {
        let mut permits = ChunkEcsPermits::default();

        for i in 0..100 {
            permits.insert(
                Entity::from_raw(i as u32),
                ChunkPos::new(0, i as i32, 0),
                Permit {
                    loadshares: LoadshareMap::default(),
                    cached_flags: PermitFlags::RENDER,
                },
            );
        }

        for i in 0..50 {
            permits
                .remove(ChunkPermitKey::Chunk(ChunkPos::new(0, i, 0)))
                .unwrap();
        }

        for i in 50..100 {
            let cpos = ChunkPos::new(0, i, 0);
            permits.get(ChunkPermitKey::Chunk(cpos)).unwrap();

            let entity = permits.get_entity(cpos).unwrap();
            assert_eq!(entity, Entity::from_raw(i as u32));
        }
    }
}
