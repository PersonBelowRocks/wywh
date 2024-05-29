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
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    topo::{
        block::{BlockVoxel, FullBlock},
        controller::LoadReasons,
        neighbors::{Neighbors, NEIGHBOR_ARRAY_SIZE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS},
    },
    util::{ivec3_to_1d, ChunkMap, ChunkSet, SyncHashMap},
};

use super::{
    chunk::ChunkFlags, Chunk, ChunkContainerError, ChunkManagerError, ChunkPos, ChunkRef,
    ChunkRefReadAccess,
};

#[derive(Default)]
pub struct LoadedChunkContainer {
    map: RwLock<ChunkMap<Chunk>>,
    force_write: AtomicBool,
}

pub struct LccRef<'a>(MappedRwLockReadGuard<'a, Chunk>);

impl<'a> std::ops::Deref for LccRef<'a> {
    type Target = Chunk;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl LoadedChunkContainer {
    pub fn new() -> Self {
        Self {
            map: RwLock::new(ChunkMap::default()),
            force_write: AtomicBool::new(false),
        }
    }

    pub fn get(&self, pos: ChunkPos) -> Result<LccRef<'_>, ChunkContainerError> {
        if self.force_write.load(Ordering::Relaxed) {
            return Err(ChunkContainerError::GloballyLocked);
        }

        let guard = self.map.read();
        if !guard.contains(pos) {
            return Err(ChunkContainerError::DoesntExist);
        }

        let chunk = RwLockReadGuard::map(guard, |g| g.get(pos).unwrap());

        Ok(LccRef(chunk))
    }

    /// Get the state of the global lock for this chunk container
    pub fn global_lock_state(&self) -> GlobalLockState {
        if self.force_write.load(Ordering::Relaxed) || self.map.is_locked_exclusive() {
            GlobalLockState::Locked
        } else {
            GlobalLockState::Unlocked
        }
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

/// Indicates what happened when we tried to load a chunk
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChunkLoadResult {
    /// The chunk we tried to load didn't exist, so it was initialized
    New,
    /// The chunk we tried to load was already loaded, so we updated its load reasons.
    /// The load reasons in this variant are the updated load reasons of the
    /// existing chunk.
    Updated(LoadReasons),
}

pub struct ChunkManagerAccess<'a> {
    chunks: &'a mut ChunkMap<Chunk>,
    statuses: RwLockWriteGuard<'a, ChunkStatuses>,
    default_block: FullBlock,
}

impl<'a> ChunkManagerAccess<'a> {
    /// Unload the chunk at the given position for the given reasons. This function will remove load reasons
    /// from the chunk and automatically unload the chunk if no reasons remain.
    /// Returns true if the chunk was unloaded, and false if not.
    pub fn unload_chunk(
        &mut self,
        pos: ChunkPos,
        unload_reasons: LoadReasons,
    ) -> Result<bool, ChunkContainerError> {
        let Some(chunk) = self.chunks.get(pos) else {
            return Err(ChunkContainerError::DoesntExist);
        };

        let mut load_reasons = chunk.load_reasons.write();
        load_reasons.remove(unload_reasons);

        if load_reasons.is_empty() {
            // Remove the chunk from the statuses
            self.statuses.fresh.remove(&pos);
            self.statuses.generating.remove(&pos);
            self.statuses.updated.remove(&pos);

            // Need to drop this immutable reference so we can mutate ourselves.
            drop(load_reasons);

            // Remove the chunk from storage
            self.chunks.remove(pos);

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn load_chunk(
        &mut self,
        pos: ChunkPos,
        load_reasons: LoadReasons,
    ) -> Result<ChunkLoadResult, ChunkManagerError> {
        match self.initialize_new_chunk(pos, load_reasons) {
            Ok(()) => Ok(ChunkLoadResult::New),
            // If the chunk is already loaded we update its load reasons by inserting the
            // reasons passed to this function
            Err(ChunkManagerError::AlreadyLoaded) => {
                let chunk = self.chunks.get(pos).expect(
                    "chunk should be present in storage because 
                        initialize_new_chunk returned AlreadyLoaded",
                );

                let mut existing_load_reasons = chunk.load_reasons.write();

                existing_load_reasons.insert(load_reasons);
                Ok(ChunkLoadResult::Updated(existing_load_reasons.clone()))
            }
            // Just forward the error to the caller, not much we can do here anyways
            Err(error) => Err(error),
        }
    }

    /// Initialize a new chunk at the given `pos` if one doesn't exist, with the provided load reasons.
    /// The chunk will be flagged as primordial. Returns `ChunkManagerError::AlreadyLoaded` if the
    /// chunk was already loaded.
    pub fn initialize_new_chunk(
        &mut self,
        pos: ChunkPos,
        load_reasons: LoadReasons,
    ) -> Result<(), ChunkManagerError> {
        if self.chunks.get(pos).is_some() {
            return Err(ChunkManagerError::AlreadyLoaded);
        }

        let chunk = Chunk::new(
            BlockVoxel::Full(self.default_block),
            ChunkFlags::PRIMORDIAL,
            load_reasons,
        );
        self.chunks.set(pos, chunk);
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum GlobalLockState {
    Locked,
    Unlocked,
}

pub struct ChunkManager {
    loaded_chunks: LoadedChunkContainer,
    status: RwLock<ChunkStatuses>,
    default_block: FullBlock,
}

impl ChunkManager {
    pub fn new(default_block: FullBlock) -> Self {
        Self {
            loaded_chunks: LoadedChunkContainer::default(),
            status: RwLock::new(ChunkStatuses::default()),
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
        let chunk = self.loaded_chunks.get(pos)?;

        if !get_primordial {
            if chunk.flags.read().contains(ChunkFlags::PRIMORDIAL) {
                return Err(ChunkManagerError::Primordial);
            }
        }

        Ok(ChunkRef {
            chunk,
            stats: self.status.read(),
            pos,
            entity: None,
        })
    }

    /// Get the chunk flags for the given chunk position
    pub fn chunk_flags(&self, pos: ChunkPos) -> Option<ChunkFlags> {
        self.get_loaded_chunk(pos, true)
            .map(|cref| cref.flags())
            .ok()
    }

    /// Get the state of the global lock
    pub fn global_lock_state(&self) -> GlobalLockState {
        self.loaded_chunks.global_lock_state()
    }

    /// Acquire a global lock of the chunk manager and its data. The close passed to this function will
    /// receive unique access to the chunk manager and be allowed to do whatever it wants without having to
    /// wait for other threads to give up their resources. This also means that this function essentially freezes
    /// everything that tries to get a chunk reference while the closure is running. You should try to complete the
    /// work you want to do in the closure as quick as possible, and try to calculate as much as possible ahead of
    /// running this function.
    /// ### Parameters
    /// You can specify a timeout to wait for, if a write lock couldn't be acquired in time, `None` will be returned.
    /// If you set `force` to true, then other threads will be prevented from getting a read lock while you're waiting
    /// for your write lock. This is essentially like getting priority over other lock consumers.
    /// ### Warning
    /// If `force` is set and this function times out, then other threads will be prevented from getting
    /// a read lock until this function succeeds again.
    pub fn with_global_lock<F, U>(&self, timeout: Option<Duration>, force: bool, f: F) -> Option<U>
    where
        F: for<'a> FnOnce(ChunkManagerAccess<'a>) -> U,
    {
        self.loaded_chunks
            .with_write_lock(timeout, force, |chunks| {
                // We get the status lock in here because the only permitted way to update statuses is
                // through a ChunkRef. If we enter this closure then there are no outstanding chunk refs,
                // thus we can assume that any thread that wanted to update statuses has done so by now.
                // There's also no need for any kind of timeout system here because status locks are never
                // held for long (unlike the chunk data lock in a chunk ref).
                let statuses = self.status.write();

                let access = ChunkManagerAccess {
                    chunks,
                    statuses,
                    default_block: self.default_block,
                };

                f(access)
            })
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

    pub fn updated_chunks(&self) -> UpdatedChunks<'_> {
        UpdatedChunks { manager: &self }
    }
}

pub struct UpdatedChunks<'a> {
    pub(super) manager: &'a ChunkManager,
}

impl<'a> UpdatedChunks<'a> {
    pub fn num_fresh_chunks(&self) -> usize {
        self.manager.status.read().fresh.len()
    }

    pub fn num_generating_chunks(&self) -> usize {
        self.manager.status.read().generating.len()
    }

    pub fn num_updated_chunks(&self) -> usize {
        self.manager.status.read().updated.len()
    }

    pub fn iter_chunks<F>(&self, mut f: F) -> Result<(), ChunkManagerError>
    where
        F: for<'cref> FnMut(ChunkRef<'cref>),
    {
        for chunk_pos in self.manager.status.read().updated.iter() {
            let cref = self.manager.get_loaded_chunk(*chunk_pos, false)?;
            f(cref);
        }

        Ok(())
    }
}
