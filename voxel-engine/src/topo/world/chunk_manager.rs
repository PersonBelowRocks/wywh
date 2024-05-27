use std::{
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use bevy::{
    ecs::entity::Entity,
    math::{ivec3, IVec3},
};
use dashmap::{mapref::one::Ref, DashSet};
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

use crate::{
    topo::{
        block::{BlockVoxel, FullBlock},
        controller::LoadReasons,
        neighbors::{Neighbors, NEIGHBOR_ARRAY_SIZE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS},
    },
    util::{ivec3_to_1d, ChunkMap, ChunkSet, SyncHashMap},
};

use super::{chunk::ChunkFlags, Chunk, ChunkManagerError, ChunkPos, ChunkRef, ChunkRefReadAccess};

#[derive(Default)]
pub struct LoadedChunkContainer {
    map: RwLock<ChunkMap<Chunk>>,
    force_write: AtomicBool,
}

pub struct LccRef<'a>(MappedRwLockReadGuard<'a, &'a Chunk>);

impl<'a> std::ops::Deref for LccRef<'a> {
    type Target = Chunk;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

pub type StrongChunkRef<'a> =
    dashmap::mapref::one::Ref<'a, ChunkPos, Arc<Chunk>, ahash::RandomState>;

impl LoadedChunkContainer {
    pub fn new() -> Self {
        Self {
            map: RwLock::new(ChunkMap::default()),
            force_write: AtomicBool::new(false),
        }
    }

    pub fn get(&self, pos: ChunkPos) -> Option<LccRef<'_>> {
        if self.force_write.load(Ordering::Relaxed) {
            return None;
        }

        let guard = self.map.read();
        if !guard.contains(pos) {
            return None;
        }

        let chunk = RwLockReadGuard::map(guard, |g| &g.get(pos).unwrap());

        Some(LccRef(chunk))
    }

    /// Get a mutable reference to the underlying map to use in a closure.
    /// You can specify a timeout to wait for, if a write lock couldn't be acquired in time, `None` will be returned.
    /// If you set `force` to true, then other threads will be prevented from getting a read lock while you're waiting
    /// for your write lock. This is essentially like getting priority over other lock consumers.
    /// IMPORTANT: if `force` is set and this function times out, then other threads will be prevented from getting
    /// a read lock until this function succeeds again.
    pub fn with_write_lock<F, U>(&self, timeout: Option<Duration>, force: bool, f: F) -> Option<U>
    where
        F: for<'a> FnOnce(&'a mut ChunkMap<Chunk>) -> U,
    {
        if force {
            self.force_write.store(true, Ordering::Release);
        }

        let mut guard = timeout
            .map(|timeout| {
                self.map.try_write_for(timeout).map(|guard| {
                    self.force_write.store(false, Ordering::Release);
                    guard
                })
            })
            .unwrap_or_else(|| {
                let guard = self.map.write();
                self.force_write.store(false, Ordering::Relaxed);
                Some(guard)
            })?;

        let mut_ref = guard.deref_mut();
        Some(f(mut_ref))
    }
}

#[derive(Default)]
pub struct PendingChunkChanges(DashSet<ChunkPos, ahash::RandomState>);

impl PendingChunkChanges {
    pub fn new() -> Self {
        Self(DashSet::with_hasher(ahash::RandomState::new()))
    }

    pub fn clear(&self) {
        self.0.clear();
    }

    pub fn has_changed(&self, pos: ChunkPos) -> bool {
        self.0.contains(&pos)
    }

    pub fn set(&self, pos: ChunkPos) {
        self.0.insert(pos);
    }

    pub fn remove(&self, pos: ChunkPos) {
        self.0.remove(&pos);
    }

    pub fn iter(&self) -> impl Iterator<Item = ChunkPos> + '_ {
        self.0.iter().map(|r| r.clone())
    }
}

#[derive(Default)]
pub struct ChunkStatuses {
    pub updated: DashSet<ChunkPos, fxhash::FxBuildHasher>,
    pub generating: DashSet<ChunkPos, fxhash::FxBuildHasher>,
    pub fresh: DashSet<ChunkPos, fxhash::FxBuildHasher>,
}

pub struct ChunkManager {
    loaded_chunks: LoadedChunkContainer,
    status: ChunkStatuses,
    default_block: FullBlock,
}

impl ChunkManager {
    pub fn new(default_block: FullBlock) -> Self {
        Self {
            loaded_chunks: LoadedChunkContainer::default(),
            status: ChunkStatuses::default(),
            default_block,
        }
    }

    /// Gets the loaded chunk at the given position if it exists, otherwise return an error.
    /// If `get_primordial` is false this function will return an error if the chunk is tagged as primordial.
    pub fn get_loaded_chunk(
        &self,
        pos: ChunkPos,
        get_primordial: bool,
    ) -> Result<ChunkRef<'_>, ChunkManagerError> {
        let chunk = self
            .loaded_chunks
            .get(pos)
            .ok_or(ChunkManagerError::Unloaded)?;

        if !get_primordial {
            if chunk.flags.read().contains(ChunkFlags::PRIMORDIAL) {
                return Err(ChunkManagerError::Primordial);
            }
        }

        Ok(ChunkRef {
            chunk,
            stats: &self.status,
            pos,
            entity: None,
        })
    }

    /// Unload a chunk from the manager.
    /// You should generally never use this method to unload chunks,
    /// instead dispatch `ChunkUnloadEvent`s in the ECS world and let
    /// the engine handle unloading for you.
    pub fn unload_chunk(&self, pos: ChunkPos) -> Result<(), ChunkManagerError> {
        if !self.has_loaded_chunk(pos) {
            return Err(ChunkManagerError::Unloaded);
        }

        // TODO: here we have a classic concurrency issue. Dashmap requires complete access to the entire
        // hashmap if we're gonna update the hashmap itself (and not just an entry/entries inside it).
        // This method may be called while the world generator is populating a chunk, or a mesh builder worker
        // is building a mesh for a chunk. In both of these scenarios there is a reference to the dashmap, meaning
        // we have to block on the 'remove' call here until there are no references anymore. This is obviously slow
        // because we're no longer generating / meshing asynchronously and our "main" thread or threads are suddenly
        // dependant on the generator and mesh builder completing their work before we can do any kind of unloading.
        // This should be fixed by reworking our concurrency model slightly, since issues like these are going to come
        // up constantly in the future we should do a proper and thorough fix now early on.
        self.loaded_chunks.remove(pos);
        self.status.fresh.remove(&pos);
        self.status.generating.remove(&pos);
        self.status.updated.remove(&pos);

        Ok(())
    }

    pub fn unload_chunks(&self, chunks: ChunkSet) {
        self.loaded_chunks.0.retain(|&pos, _| !chunks.contains(pos));
        self.status.fresh.retain(|&pos| !chunks.contains(pos));
        self.status.generating.retain(|&pos| !chunks.contains(pos));
        self.status.updated.retain(|&pos| !chunks.contains(pos));
    }

    pub fn chunk_flags(&self, pos: ChunkPos) -> Option<ChunkFlags> {
        self.get_loaded_chunk(pos, true)
            .map(|cref| cref.flags())
            .ok()
    }

    pub fn has_loaded_chunk(&self, pos: ChunkPos) -> bool {
        self.loaded_chunks.get(pos).is_some()
    }

    // TODO: test
    pub fn with_neighbors<F, R>(&self, pos: ChunkPos, mut f: F) -> Result<R, ChunkManagerError>
    where
        F: for<'a> FnMut(Neighbors<'a>) -> R,
    {
        // we need to make a map of the neighboring chunks so that the references are owned by the function scope
        let mut refs =
            std::array::from_fn::<Option<ChunkRef>, { NEIGHBOR_ARRAY_SIZE }, _>(|_| None);

        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let nbrpos = ivec3(x, y, z);
                    if nbrpos == IVec3::ZERO {
                        continue;
                    }

                    let nbrpos_ws = ChunkPos::from(nbrpos + IVec3::from(pos));
                    if let Ok(chunk_ref) = self.get_loaded_chunk(nbrpos_ws, false) {
                        refs[ivec3_to_1d(nbrpos + IVec3::ONE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS)
                            .unwrap()] = Some(chunk_ref)
                    }
                }
            }
        }

        let mut accesses = std::array::from_fn(|_| None);

        for i in 0..NEIGHBOR_ARRAY_SIZE {
            let Some(cref) = &refs[i] else {
                continue;
            };

            accesses[i] = Some(ChunkRefReadAccess {
                block_variants: cref.chunk.variants.read_access(),
            });
        }

        let neighbors = Neighbors::from_raw(accesses, BlockVoxel::Full(self.default_block));
        let result = f(neighbors);

        drop(refs);

        Ok(result)
    }

    pub fn set_loaded_chunk(&self, pos: ChunkPos, chunk: Chunk) {
        self.loaded_chunks.set(pos, chunk);
    }

    /// Initialize a new chunk at the given `pos` if one doesn't exist, with the provided load reasons.
    /// The chunk will be flagged as primordial.
    pub fn initialize_new_chunk(
        &self,
        pos: ChunkPos,
        load_reasons: LoadReasons,
    ) -> Result<(), ChunkManagerError> {
        if self.loaded_chunks.get(pos).is_some() {
            return Err(ChunkManagerError::AlreadyInitialized);
        }

        let chunk = Chunk::new(
            BlockVoxel::Full(self.default_block),
            ChunkFlags::PRIMORDIAL,
            load_reasons,
        );
        self.loaded_chunks.set(pos, chunk);
        Ok(())
    }

    pub fn updated_chunks(&self) -> UpdatedChunks<'_> {
        UpdatedChunks { manager: &self }
    }
}

pub struct UpdatedChunks<'a> {
    pub(super) manager: &'a ChunkManager,
}

impl<'a> UpdatedChunks<'a> {
    pub fn num_fresh_chunks(&self) -> usize {
        self.manager.status.fresh.len()
    }

    pub fn num_generating_chunks(&self) -> usize {
        self.manager.status.generating.len()
    }

    pub fn num_updated_chunks(&self) -> usize {
        self.manager.status.updated.len()
    }

    pub fn iter_chunks<F>(&self, mut f: F) -> Result<(), ChunkManagerError>
    where
        F: for<'cref> FnMut(ChunkRef<'cref>),
    {
        for chunk_pos in self.manager.status.updated.iter() {
            let cref = self.manager.get_loaded_chunk(*chunk_pos, false)?;
            f(cref);
        }

        Ok(())
    }
}
