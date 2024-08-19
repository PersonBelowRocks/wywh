use bevy::log::warn;
use std::{
    mem,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use bevy::math::{ivec3, IVec3};
use dashmap::DashSet;
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};

use crate::topo::world::chunk::ChunkLoadReasons;
use crate::util::ChunkSet;
use crate::{
    data::registries::block::BlockVariantId,
    topo::controller::{LoadshareId, LoadshareMap},
};
use crate::{
    topo::{
        block::{BlockVoxel, FullBlock},
        controller::LoadReasons,
        neighbors::{Neighbors, NEIGHBOR_ARRAY_SIZE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS},
    },
    util::{ivec3_to_1d, ChunkMap},
};

use super::{
    chunk::{ChunkFlags, LockStrategy},
    Chunk, ChunkContainerError, ChunkManagerError, ChunkPos, ChunkRef,
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
    pub fn with_write_lock<F>(&self, timeout: Option<Duration>, force: bool, f: F) -> bool
    where
        F: for<'a> FnOnce(&'a mut ChunkMap<Chunk>),
    {
        if force {
            self.force_write.store(true, Ordering::Release);
        }

        let guard = timeout
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
            });

        match guard {
            Some(mut guard) => {
                let c = guard.deref_mut();
                f(c);
                true
            }
            None => false,
        }
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

/// Indicates what happened when we tried to unload a chunk
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChunkUnloadResult {
    /// The chunk was completely unloaded
    Unloaded,
    /// The chunk's load reasons were updated but it was not unloaded
    ReasonsUpdated,
    /// The chunk was unloaded from the loadshare but is still loaded under another loadshare
    UnloadedInLoadshare,
}

/// The chunks loaded under a loadshare
pub struct LoadshareChunks<'a> {
    loadshare: LoadshareId,
    default_block: BlockVariantId,
    loadshare_chunks: &'a mut ChunkSet,
    statuses: &'a mut ChunkStatuses,
    chunks: &'a mut ChunkMap<Chunk>,
}

impl<'a> LoadshareChunks<'a> {
    /// Unload the chunk at the given position for the given reasons. This function will remove load reasons
    /// from the chunk and automatically unload the chunk if no reasons remain.
    /// Returns true if the chunk was unloaded, and false if not.
    pub fn unload_chunk(
        &mut self,
        pos: ChunkPos,
        unload_reasons: LoadReasons,
    ) -> Result<ChunkUnloadResult, ChunkContainerError> {
        let Some(chunk) = self.chunks.get(pos) else {
            return Err(ChunkContainerError::DoesntExist);
        };

        let mut load_reasons = chunk.load_reasons.write();
        let loadshare_load_reasons = load_reasons
            .loadshares
            .get_mut(&self.loadshare)
            .ok_or(ChunkContainerError::InvalidLoadshare)?;

        loadshare_load_reasons.remove(unload_reasons);
        // Make a copy so we can drop our mutable reference and update the cached reasons
        let reasons = *loadshare_load_reasons;
        load_reasons.update_cached_reasons();

        // If there are no more load reasons under this loadshare, we can unload this chunk from this loadshare
        if reasons.is_empty() {
            if load_reasons.cached_reasons.is_empty() {
                // Remove the chunk from the updated chunks
                self.statuses.updated.remove(&pos);

                // Need to drop this immutable reference so we can mutate ourselves.
                drop(load_reasons);

                // Remove the chunk from storage
                self.chunks.remove(pos);
                self.loadshare_chunks.remove(pos);

                Ok(ChunkUnloadResult::Unloaded)
            } else {
                load_reasons.loadshares.remove(&self.loadshare);
                self.loadshare_chunks.remove(pos);

                Ok(ChunkUnloadResult::UnloadedInLoadshare)
            }
        } else {
            Ok(ChunkUnloadResult::ReasonsUpdated)
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
            // reasons passed to this function under our current loadshare
            Err(ChunkManagerError::AlreadyLoaded) => {
                let chunk = self.chunks.get(pos).expect(
                    "chunk should be present in storage because
                        initialize_new_chunk returned AlreadyLoaded",
                );

                let mut existing_load_reasons = chunk.load_reasons.write();
                existing_load_reasons
                    .loadshares
                    .entry(self.loadshare)
                    .and_modify(|reasons| reasons.insert(load_reasons))
                    .or_insert_with(|| {
                        self.loadshare_chunks.set(pos);
                        load_reasons
                    });

                existing_load_reasons.update_cached_reasons();
                Ok(ChunkLoadResult::Updated(
                    existing_load_reasons.cached_reasons.clone(),
                ))
            }
            // Just forward the error to the caller, not much we can do here anyway
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
            self.default_block,
            ChunkFlags::PRIMORDIAL,
            ChunkLoadReasons {
                loadshares: LoadshareMap::from_iter([(self.loadshare, load_reasons)]),
                cached_reasons: load_reasons,
            },
        );
        self.chunks.set(pos, chunk);
        Ok(())
    }

    /// Unload all chunks under this loadshare but don't remove the loadshare itself from the map tracking
    /// the chunks that loadshares own. This leaves the chunk manager in a broken state and this must
    /// be resolved manually after running this function
    pub(crate) fn partially_unload_all_chunks(&mut self) {
        let chunks = mem::replace(self.loadshare_chunks, ChunkSet::default());
        for chunk_pos in chunks.into_iter() {
            if let Err(error) = self.unload_chunk(chunk_pos, LoadReasons::all()) {
                warn!("Error unloading chunk {chunk_pos} as part of a complete loadshare removal: {error}");
                continue;
            }
        }
    }
}

pub struct ChunkManagerAccess<'a> {
    chunks: &'a mut ChunkMap<Chunk>,
    loadshares: &'a mut LoadshareMap<ChunkSet>,
    statuses: &'a mut ChunkStatuses,
    default_block: BlockVariantId,
}

impl<'a> ChunkManagerAccess<'a> {
    pub fn loadshare(&mut self, loadshare: LoadshareId) -> LoadshareChunks<'_> {
        LoadshareChunks {
            loadshare,
            loadshare_chunks: self
                .loadshares
                .entry(loadshare)
                .or_insert(ChunkSet::default()),
            chunks: self.chunks,
            statuses: &mut self.statuses,
            default_block: self.default_block,
        }
    }

    pub fn remove_loadshare(&mut self, loadshare: LoadshareId) {
        let mut loadshare_chunks = self.loadshare(loadshare);

        loadshare_chunks.partially_unload_all_chunks();

        self.loadshares.remove(&loadshare);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum GlobalLockState {
    Locked,
    Unlocked,
}

pub struct ChunkManager {
    loaded_chunks: LoadedChunkContainer,
    loadshares: RwLock<LoadshareMap<ChunkSet>>,
    status: RwLock<ChunkStatuses>,
    default_block: BlockVariantId,
}

impl ChunkManager {
    pub fn new(default_block: BlockVariantId) -> Self {
        Self {
            loaded_chunks: LoadedChunkContainer::default(),
            loadshares: RwLock::new(LoadshareMap::default()),
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
    ) -> Result<LccRef<'_>, ChunkManagerError> {
        let chunk = self.loaded_chunks.get(pos)?;

        if !get_primordial && chunk.flags.read().contains(ChunkFlags::PRIMORDIAL) {
            return Err(ChunkManagerError::Primordial);
        }

        Ok(chunk)
    }

    /// Get the chunk flags for the given chunk position. This function is basically a shorthand for
    /// getting the chunk and checking the flags manually. If you need more control over this whole
    /// operation (like using different locking strategies), then you should do that instead.
    pub fn chunk_flags(&self, pos: ChunkPos) -> Option<ChunkFlags> {
        self.get_loaded_chunk(pos, true)
            .map(|cref| cref.flags(LockStrategy::Blocking).unwrap())
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
    pub fn with_global_lock<F>(&self, timeout: Option<Duration>, force: bool, f: F) -> bool
    where
        F: for<'a> FnOnce(ChunkManagerAccess<'a>),
    {
        // TODO: this should be a DashMap (or some other concurrent data structure) and the global lock
        //  should be a separate field or something (like an RwLock<()>) so that we can manage chunks
        //  concurrently
        self.loaded_chunks
            .with_write_lock(timeout, force, |chunks| {
                // We get the status lock in here because the only permitted way to update statuses is
                // through a ChunkRef. If we enter this closure then there are no outstanding chunk refs,
                // thus we can assume that any thread that wanted to update statuses has done so by now.
                // There's also no need for any kind of timeout system here because status locks are never
                // held for long (unlike the chunk data lock in a chunk ref).
                let mut statuses = self.status.write();
                let mut loadshares = self.loadshares.write();

                let access = ChunkManagerAccess {
                    chunks,
                    loadshares: &mut loadshares,
                    statuses: &mut statuses,
                    default_block: self.default_block,
                };

                f(access);
            })
    }

    // TODO: test
    pub fn with_neighbors<F, R>(
        &self,
        pos: ChunkPos,
        strategy: LockStrategy,
        mut f: F,
    ) -> Result<R, ChunkManagerError>
    where
        F: for<'a> FnMut(Neighbors<'a>) -> R,
    {
        // we need to make a map of the neighboring chunks so that the references are owned by the function scope
        let mut refs = std::array::from_fn::<Option<LccRef>, { NEIGHBOR_ARRAY_SIZE }, _>(|_| None);

        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let nbrpos = ivec3(x, y, z);
                    if nbrpos == IVec3::ZERO {
                        continue;
                    }

                    let nbrpos_ws = ChunkPos::from(nbrpos + IVec3::from(pos));
                    if let Ok(chunk) = self.get_loaded_chunk(nbrpos_ws, false) {
                        refs[ivec3_to_1d(nbrpos + IVec3::ONE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS)
                            .unwrap()] = Some(chunk)
                    }
                }
            }
        }

        let mut handles = std::array::from_fn(|_| None);

        for i in 0..NEIGHBOR_ARRAY_SIZE {
            let Some(cref) = &refs[i] else {
                continue;
            };

            handles[i] = Some(cref.read_handle(strategy)?);
        }

        let neighbors = Neighbors::from_raw(handles, self.default_block);
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
    pub fn num_updated_chunks(&self) -> usize {
        self.manager.status.read().updated.len()
    }

    pub fn iter_chunks<F>(&self, mut f: F) -> Result<(), ChunkManagerError>
    where
        F: for<'cref> FnMut(LccRef<'cref>),
    {
        for chunk_pos in self.manager.status.read().updated.iter() {
            let cref = self.manager.get_loaded_chunk(*chunk_pos, false)?;
            f(cref);
        }

        Ok(())
    }
}
