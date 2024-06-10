use bevy::ecs::entity::{Entity, EntityHashMap};

use dashmap::{
    mapref::{entry::Entry as DashMapEntry, one::Ref as DashMapRef},
    DashMap,
};

use hb::hash_map::{Drain, Entry as HashbrownEntry};
use indexmap::IndexMap;
use itertools::Itertools;

use crate::topo::world::ChunkPos;

pub type ChunkIndexMap<T> = IndexMap<ChunkPos, T, wyhash2::WyHash>;

#[derive(Clone)]
pub struct MultiChunkMapEntry<T> {
    data: T,
    entity: Entity,
    chunk_pos: ChunkPos,
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub enum MultiChunkMapKey {
    Entity(Entity),
    Chunk(ChunkPos),
}

#[derive(Clone)]
pub struct MultiChunkMap<T> {
    entities: EntityHashMap<usize>,
    chunks: ChunkMap<usize>,

    data: Vec<MultiChunkMapEntry<T>>,
}

impl<T> MultiChunkMap<T> {
    pub fn new() -> Self {
        Self {
            entities: EntityHashMap::default(),
            chunks: ChunkMap::default(),
            data: Vec::new(),
        }
    }

    fn get_index(&self, key: MultiChunkMapKey) -> Option<usize> {
        match key {
            MultiChunkMapKey::Chunk(chunk_pos) => self.chunks.get(chunk_pos).copied(),
            MultiChunkMapKey::Entity(entity) => self.entities.get(&entity).copied(),
        }
    }

    /// Get the entity associated with a chunk position.
    pub fn get_entity(&self, chunk_pos: ChunkPos) -> Option<Entity> {
        let idx = self.get_index(MultiChunkMapKey::Chunk(chunk_pos))?;

        Some(self.data[idx].entity)
    }

    /// Get the chunk position associated with an entity.
    pub fn get_chunk(&self, entity: Entity) -> Option<ChunkPos> {
        let idx = self.get_index(MultiChunkMapKey::Entity(entity))?;

        Some(self.data[idx].chunk_pos)
    }

    /// Checks if this multi chunk map contains a key.
    pub fn contains(&self, key: MultiChunkMapKey) -> bool {
        self.get_index(key).is_some()
    }

    /// Checks if the provided entity and chunk pos are tied (aka. point to the same data) in
    /// this multi chunk map.
    pub fn tied(&self, entity: Entity, chunk_pos: ChunkPos) -> bool {
        self.get_chunk(entity).is_some_and(|pos| pos == chunk_pos)
    }

    /// Get an immutable reference to the data at the given key.
    pub fn get(&self, key: MultiChunkMapKey) -> Option<&T> {
        let idx = self.get_index(key)?;
        Some(&self.data[idx].data)
    }

    /// Get a mutable reference to the data at the given key.
    pub fn get_mut(&mut self, key: MultiChunkMapKey) -> Option<&mut T> {
        let idx = self.get_index(key)?;
        Some(&mut self.data[idx].data)
    }

    pub fn insert(&mut self, entity: Entity, chunk_pos: ChunkPos, data: T) {
        // Remove anything tied to the entity or chunk position that we had before.
        self.remove(MultiChunkMapKey::Entity(entity));
        self.remove(MultiChunkMapKey::Chunk(chunk_pos));

        // The length of our data vector before we push to it is the index
        // of the data entry that we're about to push.
        let idx = self.data.len();
        self.data.push(MultiChunkMapEntry {
            data,
            entity,
            chunk_pos,
        });

        // Tie the entity and chunks to the index of the data in our data vector.
        self.entities.insert(entity, idx);
        self.chunks.set(chunk_pos, idx);
    }

    pub fn remove(&mut self, key: MultiChunkMapKey) -> Option<T> {
        let idx = self.get_index(key)?;

        let removed = self.data.swap_remove(idx);

        self.entities.remove(&removed.entity);
        self.chunks.remove(removed.chunk_pos);

        // If our index is equal to our length, then we removed the last element in our data vector, and
        // we didn't move the last element in the vector into the removed index. In this case we don't
        // want to update anything in the vector.
        if idx < self.data.len() {
            let moved = &self.data[idx];

            // Some sanity checks to make sure invariants aren't broken
            debug_assert!(self.entities.contains_key(&moved.entity));
            debug_assert!(self.chunks.contains(moved.chunk_pos));

            self.entities.entry(moved.entity).and_modify(|i| *i = idx);
            self.chunks.entry(moved.chunk_pos).and_modify(|i| *i = idx);
        }

        Some(removed.data)
    }
}

#[derive(Clone, Default, Debug)]
pub struct ChunkSet(hb::HashSet<ChunkPos, wyhash2::WyHash>);

impl ChunkSet {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(hb::HashSet::with_capacity_and_hasher(
            capacity,
            wyhash2::WyHash::default(),
        ))
    }

    pub fn set(&mut self, pos: ChunkPos) -> bool {
        self.0.insert(pos)
    }

    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.0.contains(&pos)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn remove(&mut self, pos: ChunkPos) -> bool {
        self.0.remove(&pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.0.iter().cloned()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn drain(&mut self) -> hb::hash_set::Drain<'_, ChunkPos> {
        self.0.drain()
    }
}

#[derive(Clone)]
pub struct SyncChunkMap<T>(DashMap<ChunkPos, T, wyhash2::WyHash>);

impl<T> Default for SyncChunkMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SyncChunkMap<T> {
    pub fn new() -> Self {
        Self(DashMap::with_hasher(wyhash2::WyHash::default()))
    }

    pub fn set(&self, pos: ChunkPos, data: T) -> Option<T> {
        self.0.insert(pos, data)
    }

    pub fn get(&self, pos: ChunkPos) -> Option<DashMapRef<ChunkPos, T, wyhash2::WyHash>> {
        self.0.get(&pos)
    }

    pub fn remove(&self, pos: ChunkPos) -> Option<T> {
        self.0.remove(&pos).map(|(_, data)| data)
    }

    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.0.contains_key(&pos)
    }

    pub fn entry(&self, pos: ChunkPos) -> DashMapEntry<'_, ChunkPos, T, wyhash2::WyHash> {
        self.0.entry(pos)
    }

    pub fn for_each_key<F>(&self, mut f: F)
    where
        F: FnMut(ChunkPos),
    {
        for entry in self.0.iter().map(|e| e) {
            f(*entry.key())
        }
    }

    pub fn keys(&self) -> Vec<ChunkPos> {
        self.0.iter().map(|e| *e.key()).collect_vec()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn shrink_to_fit(&self) {
        self.0.shrink_to_fit()
    }
}

#[derive(Clone)]
pub struct ChunkMap<T>(hb::HashMap<ChunkPos, T, wyhash2::WyHash>);

impl<T> Default for ChunkMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ChunkMap<T> {
    pub fn new() -> Self {
        Self(hb::HashMap::with_hasher(wyhash2::WyHash::default()))
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(hb::HashMap::with_capacity_and_hasher(
            capacity,
            wyhash2::WyHash::default(),
        ))
    }

    pub fn set(&mut self, pos: ChunkPos, data: T) -> Option<T> {
        self.0.insert(pos, data)
    }

    pub fn get(&self, pos: ChunkPos) -> Option<&T> {
        self.0.get(&pos)
    }

    pub fn get_mut(&mut self, pos: ChunkPos) -> Option<&mut T> {
        self.0.get_mut(&pos)
    }

    pub fn remove(&mut self, pos: ChunkPos) -> Option<T> {
        self.0.remove(&pos)
    }

    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.0.contains_key(&pos)
    }

    pub fn entry(&mut self, pos: ChunkPos) -> HashbrownEntry<'_, ChunkPos, T, wyhash2::WyHash> {
        self.0.entry(pos)
    }

    pub fn for_each_pos<F>(&self, mut f: F)
    where
        F: FnMut(ChunkPos),
    {
        for &key in self.0.keys() {
            f(key)
        }
    }

    pub fn for_each_entry_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(ChunkPos, &mut T),
    {
        for (&pos, item) in self.0.iter_mut() {
            f(pos, item)
        }
    }

    pub fn for_each_entry<F>(&self, mut f: F)
    where
        F: FnMut(ChunkPos, &T),
    {
        for (&pos, item) in self.0.iter() {
            f(pos, item)
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ChunkPos, &T)> {
        self.0.iter().map(|(&pos, data)| (pos, data))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (ChunkPos, &mut T)> {
        self.0.iter_mut().map(|(&pos, data)| (pos, data))
    }

    pub fn into_iter(self) -> impl Iterator<Item = (ChunkPos, T)> {
        self.0.into_iter()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn drain(&mut self) -> Drain<'_, ChunkPos, T> {
        self.0.drain()
    }
}

impl<T> Extend<(ChunkPos, T)> for ChunkMap<T> {
    fn extend<I: IntoIterator<Item = (ChunkPos, T)>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}
