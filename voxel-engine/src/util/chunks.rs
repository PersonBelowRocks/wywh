use dashmap::{
    mapref::{entry::Entry as DashMapEntry, one::Ref as DashMapRef},
    DashMap,
};
use fxhash::FxBuildHasher;
use hb::hash_map::Entry as HashbrownEntry;
use itertools::Itertools;

use crate::topo::world::ChunkPos;

#[derive(Clone, Default, Debug)]
pub struct ChunkSet(hb::HashSet<ChunkPos, fxhash::FxBuildHasher>);

impl ChunkSet {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(hb::HashSet::with_capacity_and_hasher(
            capacity,
            fxhash::FxBuildHasher::default(),
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

    pub fn remove(&mut self, pos: ChunkPos) -> bool {
        self.0.remove(&pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.0.iter().cloned()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }
}

#[derive(Clone)]
pub struct SyncChunkMap<T>(DashMap<ChunkPos, T, fxhash::FxBuildHasher>);

impl<T> Default for SyncChunkMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SyncChunkMap<T> {
    pub fn new() -> Self {
        Self(DashMap::with_hasher(FxBuildHasher::default()))
    }

    pub fn set(&self, pos: ChunkPos, data: T) -> Option<T> {
        self.0.insert(pos, data)
    }

    pub fn get(&self, pos: ChunkPos) -> Option<DashMapRef<ChunkPos, T, fxhash::FxBuildHasher>> {
        self.0.get(&pos)
    }

    pub fn remove(&self, pos: ChunkPos) -> Option<T> {
        self.0.remove(&pos).map(|(_, data)| data)
    }

    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.0.contains_key(&pos)
    }

    pub fn entry(&self, pos: ChunkPos) -> DashMapEntry<'_, ChunkPos, T, fxhash::FxBuildHasher> {
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
pub struct ChunkMap<T>(hb::HashMap<ChunkPos, T, fxhash::FxBuildHasher>);

impl<T> Default for ChunkMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ChunkMap<T> {
    pub fn new() -> Self {
        Self(hb::HashMap::with_hasher(fxhash::FxBuildHasher::default()))
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(hb::HashMap::with_capacity_and_hasher(
            capacity,
            fxhash::FxBuildHasher::default(),
        ))
    }

    pub fn set(&mut self, pos: ChunkPos, data: T) -> Option<T> {
        self.0.insert(pos, data)
    }

    pub fn get(&self, pos: ChunkPos) -> Option<&T> {
        self.0.get(&pos)
    }

    pub fn remove(&mut self, pos: ChunkPos) -> Option<T> {
        self.0.remove(&pos)
    }

    pub fn contains(&self, pos: ChunkPos) -> bool {
        self.0.contains_key(&pos)
    }

    pub fn entry(
        &mut self,
        pos: ChunkPos,
    ) -> HashbrownEntry<'_, ChunkPos, T, fxhash::FxBuildHasher> {
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

    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ChunkPos, &T)> {
        self.0.iter().map(|(&pos, data)| (pos, data))
    }
}
