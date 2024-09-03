use std::{ops::Range, sync::Arc};

use bevy::math::{ivec3, IVec3};
use dashmap::{mapref::entry::Entry as DashMapEntry, DashMap, DashSet};
use error::{ChunkGetError, ChunkLoadError, CmStructuralError};
use hb::{hash_map::Entry, HashMap};
use inner_storage::{ChunkStorageHasher, InnerChunkStorage, LoadedChunk};
use itertools::Itertools;
use parking_lot::{Mutex, MutexGuard, RwLock};

use crate::{
    data::registries::block::BlockVariantId,
    topo::{
        controller::{LoadReasons, LoadshareId, LoadshareIdHasher, LoadshareMap},
        neighbors::{Neighbors, NeighborsBuilder},
    },
    util::{
        sync::{LockStrategy, StrategicWriteLock, StrategySyncError},
        ChunkSet,
    },
};

use super::{chunk::ChunkFlags, Chunk, ChunkPos, ChunkRef};

pub mod ecs;
/// Errors related to chunk management.
pub mod error;
mod inner_storage;

/// The vertical bounds of the world. Chunk positions must have their Y within this range.
pub const WORLD_VERTICAL_DIMENSIONS: Range<i32> = -2048..2048;

/// The horizontal bounds of the world. Chunk positions must have their X and Z within this range.
pub const WORLD_HORIZONTAL_DIMENSIONS: Range<i32> = -65536..65536;

/// Check if a chunk position is in bounds for the world size.
#[inline]
pub fn chunk_pos_in_bounds(chunk_pos: ChunkPos) -> bool {
    let [x, y, z] = chunk_pos.as_ivec3().to_array();

    WORLD_HORIZONTAL_DIMENSIONS.contains(&x)
        && WORLD_HORIZONTAL_DIMENSIONS.contains(&z)
        && WORLD_VERTICAL_DIMENSIONS.contains(&y)
}

/// Sets of loaded chunks with certain properties.
#[derive(Default)]
pub struct ChunkStatuses {
    /// Chunks that need remeshing.
    pub remesh: DashSet<ChunkPos, ChunkStorageHasher>,
    /// Chunks that are completely solid.
    pub solid: DashSet<ChunkPos, ChunkStorageHasher>,
}

/// The chunk manager stores and manages the lifecycle of chunks.
pub struct ChunkManager {
    default_block: BlockVariantId,
    storage: InnerChunkStorage,
    statuses: ChunkStatuses,
    loadshares: ChunkLoadshareTable,
    structural_lock: Mutex<()>,
}

/// A "write" handle to the chunk manager, allowing for structural changes related to chunk lifecycle management.
pub struct ChunkStorageStructure<'a> {
    guard: MutexGuard<'a, ()>,
    /// All loaded chunks.
    pub loaded: &'a DashMap<ChunkPos, LoadedChunk, ChunkStorageHasher>,
    /// A temporary stop for chunks before they are unloaded. Chunks in purgatory should not
    /// be modified, but they may be revived and moved back to the loaded state.
    pub purgatory: &'a DashMap<ChunkPos, Arc<Chunk>, ChunkStorageHasher>,
    /// A map of all chunks and the loadshares they are loaded under with the reasons why.
    pub loadshares_for_chunks: &'a DashMap<ChunkPos, ChunkLoadshares, ChunkStorageHasher>,
    /// A map of all loadshares and the chunks they have loaded (for any reason).
    pub chunks_for_loadshares: &'a DashMap<LoadshareId, ChunkSet, LoadshareIdHasher>,
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Chunk Manager Structural Access
///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// Describes details of how a chunk was loaded
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChunkLoadResult {
    /// The chunk was revived from purgatory.
    Revived,
    /// The chunk was freshly loaded.
    New,
}

impl<'a> ChunkStorageStructure<'a> {
    /// Returns `true` if the chunk is present in [`Self::loaded`].
    pub fn is_loaded(&self, chunk_pos: ChunkPos) -> bool {
        self.loaded.contains_key(&chunk_pos)
    }

    /// Returns `true` if the chunk is present in [`Self::purgatory`].
    pub fn is_purged(&self, chunk_pos: ChunkPos) -> bool {
        self.purgatory.contains_key(&chunk_pos)
    }

    /// Get the union of all load reasons across all loadshares for this chunk.
    /// If the result is empty (i.e., no load reasons), the chunk should be purged.
    /// Returns [`CmStructuralError::NotLoaded`] if the chunk was not loaded.
    pub fn load_reasons_union(
        &self,
        chunk_pos: ChunkPos,
    ) -> Result<LoadReasons, CmStructuralError> {
        self.loaded
            .get(&chunk_pos)
            .map(|loaded_chunk| loaded_chunk.cached_loadshare_reasons_union)
            .ok_or(CmStructuralError::NotLoaded)
    }

    /// Place a chunk under a loadshare for the given `load_reasons`. This operates on already loaded chunks and will not load any chunk.
    /// - If the chunk was loaded, but not under this loadshare, it will be placed under this loadshare for the given reasons.
    /// - If the chunk was loaded under this loadshare, that loadshare's load reasons will have the given `load_reasons` added to them.
    ///
    /// Returns an error if:
    /// - The chunk was not loaded
    /// - The provided load reasons were empty.
    pub fn add_loadshare_load_reasons(
        &self,
        chunk_pos: ChunkPos,
        loadshare: LoadshareId,
        load_reasons: LoadReasons,
    ) -> Result<(), CmStructuralError> {
        let mut loaded_chunk = self
            .loaded
            .get_mut(&chunk_pos)
            .ok_or(CmStructuralError::NotLoaded)?;

        if load_reasons.is_empty() {
            return Err(CmStructuralError::NoLoadReasons);
        }

        // Add this loadshare to the chunk.
        let loadshares = self
            .loadshares_for_chunks
            .entry(chunk_pos)
            .and_modify(|loadshares| loadshares.insert(loadshare, load_reasons))
            .or_insert(ChunkLoadshares::single(loadshare, load_reasons));

        // Update the cached load reasons.
        let load_reasons = loadshares.load_reasons_union();
        loaded_chunk.cached_loadshare_reasons_union = load_reasons;

        // Add this chunk to the loadshare.
        self.chunks_for_loadshares
            .entry(loadshare)
            .and_modify(|chunks| {
                chunks.set(chunk_pos);
            })
            .or_insert_with(|| ChunkSet::single(chunk_pos));

        Ok(())
    }

    /// Remove the given `load_reasons` from a loadshare's load reasons for a chunk.
    /// If the loadshare has no load reasons for this chunk after the given `load_reasons` were removed, the chunk is removed from that loadshare.
    /// Returns an error if:
    /// - The chunk was not loaded under at all
    /// - The chunk was not loaded under the given loadshare
    /// - The provided load reasons were empty
    ///
    /// # Important
    /// If the union of all load reasons for this chunk is empty, the chunk must be purged. Callers must do this manually after running this function.
    pub fn remove_loadshare_load_reasons(
        &self,
        chunk_pos: ChunkPos,
        loadshare: LoadshareId,
        load_reasons: LoadReasons,
    ) -> Result<(), CmStructuralError> {
        let mut loaded_chunk = self
            .loaded
            .get_mut(&chunk_pos)
            .ok_or(CmStructuralError::NotLoaded)?;

        // We're not adding any new load reasons so we can skip everything else.
        if load_reasons.is_empty() {
            return Err(CmStructuralError::NoLoadReasons);
        }

        // If a chunk is loaded it must be loaded under at least one loadshare.
        let mut loadshares = self
            .loadshares_for_chunks
            .get_mut(&chunk_pos)
            .ok_or(CmStructuralError::NotInLoadshare)?;

        let result = loadshares.remove(loadshare, load_reasons);
        if result == LoadshareRemovalResult::LoadshareRemoved {
            if let DashMapEntry::Occupied(mut entry) = self.chunks_for_loadshares.entry(loadshare) {
                entry.get_mut().remove(chunk_pos);
                // If there are no more chunks loaded under this loadshare, then just remove the whole loadshare.
                if entry.get_mut().is_empty() {
                    entry.remove();
                }
            }
        } else if result == LoadshareRemovalResult::NoLoadshare {
            return Err(CmStructuralError::NotInLoadshare);
        }

        let cached_load_reasons = loadshares.load_reasons_union();
        loaded_chunk.cached_loadshare_reasons_union = cached_load_reasons;

        Ok(())
    }

    /// Load or revive a chunk. Returns [`ChunkLoadResult`] which indicates if the chunk was revived or loaded.
    /// Returns an error if the chunk was already loaded.
    /// The loaded chunk will have no load reasons and will not be under any loadshares, it's up to the caller to sort this out after calling this function.
    /// Chunks should not remain loaded without loadshares and/or load reasons.
    pub fn load_chunk(&self, chunk: Chunk) -> Result<ChunkLoadResult, CmStructuralError> {
        if self.loaded.contains_key(&chunk.chunk_pos()) {
            return Err(CmStructuralError::ChunkAlreadyLoaded);
        }

        let mut result = ChunkLoadResult::New;

        // Revive the chunk from purgatory if possible, otherwise create a new one.
        // FIXME: it seems that reviving a chunk doesn't play nicely with meshes, we
        //  should probably handle this a bit more elegantly
        let chunk = self
            .purgatory
            .remove(&chunk.chunk_pos())
            .inspect(|_| result = ChunkLoadResult::Revived)
            .unwrap_or_else(|| (chunk.chunk_pos(), Arc::new(chunk)))
            .1;

        self.loaded.insert(
            chunk.chunk_pos(),
            LoadedChunk {
                chunk,
                // Caller must set load reasons manually
                cached_loadshare_reasons_union: LoadReasons::empty(),
            },
        );

        Ok(result)
    }

    /// Purge a chunk. Returns [`CmStructuralError::NotLoaded`] if the chunk was not loaded or was already purged.
    /// Will wipe this chunk from all loadshares.
    pub fn purge_chunk(&self, chunk_pos: ChunkPos) -> Result<(), CmStructuralError> {
        let purged_chunk = self
            .loaded
            .remove(&chunk_pos)
            .ok_or(CmStructuralError::NotLoaded)?
            .1;

        // Remove this chunk from all loadshares
        self.loadshares_for_chunks
            .remove(&chunk_pos)
            .map(|(_, loadshares)| {
                for loadshare in loadshares.iter_loadshares() {
                    let DashMapEntry::Occupied(mut entry) =
                        self.chunks_for_loadshares.entry(loadshare)
                    else {
                        continue;
                    };

                    // Remove this chunk from this loadshare, and if there are no more chunks in the loadshare, remove the entire loadshare.
                    entry.get_mut().remove(chunk_pos);
                    if entry.get().is_empty() {
                        entry.remove();
                    }
                }
            });

        self.purgatory.insert(chunk_pos, purged_chunk.chunk);

        Ok(())
    }
}

///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// Chunk Manager
///////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

impl ChunkManager {
    /// Create a new chunk manager with a default block.
    pub fn new(default_block: BlockVariantId) -> Self {
        Self {
            structural_lock: Mutex::default(),
            default_block,
            storage: InnerChunkStorage::default(),
            statuses: ChunkStatuses::default(),
            loadshares: ChunkLoadshareTable::default(),
        }
    }

    /// Get a handle to perform structural changes to the chunk storage (i.e., loading and purging chunks).
    /// Returns an error depending on the [locking strategy][].
    ///
    /// This is a very low-level API and should not be used outside of engine internals.
    ///
    /// [locking strategy]: LockStrategy
    #[inline]
    pub fn structural_access<F>(
        &self,
        strategy: LockStrategy,
        callback: F,
    ) -> Result<(), StrategySyncError>
    where
        F: for<'a> FnOnce(ChunkStorageStructure<'a>),
    {
        let guard = self.structural_lock.strategic_write(strategy)?;

        callback(ChunkStorageStructure {
            guard,
            loaded: &self.storage.loaded,
            purgatory: &self.storage.purgatory,
            loadshares_for_chunks: &self.loadshares.loadshares_for_chunks,
            chunks_for_loadshares: &self.loadshares.chunks_for_loadshares,
        });

        Ok(())
    }

    /// Whether the given chunk is loaded or not.
    #[inline]
    pub fn is_loaded(&self, chunk_pos: ChunkPos) -> bool {
        self.storage.is_loaded(chunk_pos)
    }

    /// Get all the solid chunks in this manager.
    #[inline]
    pub fn solid_chunks(&self) -> Vec<ChunkPos> {
        self.statuses
            .solid
            .iter()
            .map(|chunk_pos| chunk_pos.clone())
            .collect_vec()
    }

    /// Get all the chunks marked for remeshing/mesh-building in this manager.
    #[inline]
    pub fn remesh_chunks(&self) -> Vec<ChunkPos> {
        self.statuses
            .remesh
            .iter()
            .map(|chunk_pos| chunk_pos.clone())
            .collect_vec()
    }

    /// Get the chunk loaded at the given position.
    #[inline(never)] // Never inline this function so that it shows up when debugging.
    pub fn loaded_chunk(&self, chunk_pos: ChunkPos) -> Result<ChunkRef<'_>, ChunkGetError> {
        if !chunk_pos_in_bounds(chunk_pos) {
            return Err(ChunkGetError::out_of_bounds(chunk_pos));
        }

        let chunk = self
            .storage
            .get(chunk_pos)
            .ok_or(ChunkGetError::NotLoaded(chunk_pos))?;

        Ok(ChunkRef {
            chunk,
            stats: &self.statuses,
        })
    }

    /// Get the flags for the given chunk blockingly.
    #[inline]
    pub fn chunk_flags(&self, chunk_pos: ChunkPos) -> Result<ChunkFlags, ChunkGetError> {
        Ok(self
            .loaded_chunk(chunk_pos)?
            .flags(LockStrategy::Blocking)
            .unwrap())
    }

    // TODO: docs
    #[inline]
    pub fn neighbors<T, F>(
        &self,
        chunk_pos: ChunkPos,
        strategy: LockStrategy,
        callback: F,
    ) -> Result<T, ChunkGetError>
    where
        F: for<'a> FnOnce(Neighbors<'a>) -> T,
    {
        // This vector will hold all our chunks while we read from them.
        let mut chunks = Vec::with_capacity(3 * 3 * 3);

        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let offset = ivec3(x, y, z);
                    // Ignore the center chunk since that's the one we're getting the neighbors for.
                    if offset == IVec3::ZERO {
                        continue;
                    }

                    let neighbor_chunk_pos = ChunkPos::from(offset + chunk_pos.as_ivec3());
                    if let Some(chunk) = self.storage.get(neighbor_chunk_pos) {
                        chunks.push((offset, chunk));
                    }
                }
            }
        }

        let mut neighbor_builder = NeighborsBuilder::new(self.default_block);
        for (offset, chunk) in chunks.iter() {
            let read_handle = chunk.read_handle(strategy).unwrap();
            neighbor_builder.set_neighbor(*offset, read_handle).unwrap();
        }

        let neighbors = neighbor_builder.build();

        Ok(callback(neighbors))
    }

    /// Create a new primordial chunk at the given position. Does not load or unload any chunks, rather
    /// this function uses the manager's settins to create a pre-configured chunk that can be loaded seperately.
    #[inline]
    pub(super) fn new_primordial_chunk(&self, chunk_pos: ChunkPos) -> Chunk {
        Chunk::new(chunk_pos, self.default_block, ChunkFlags::PRIMORDIAL)
    }
}

/// A chunk's loadshare(s). If a chunk is only loaded under one loadshare (very common), this data will
/// be stored inline to avoid unnecessary allocations.
#[derive(Clone)]
pub struct ChunkLoadshares(ChunkLoadsharesInner);

/// The inner enum for [`ChunkLoadshares`]. This is private so that users can't mess with the enum variants since
/// there's some rules we'd like to enforce for those.
#[derive(Clone)]
enum ChunkLoadsharesInner {
    /// No loadshares or load reasons. This variant should only be encountered temporarily and
    /// indicates that a chunk should be purged.
    Empty,
    /// Chunk is loaded under a single loadshare.
    Single {
        loadshare: LoadshareId,
        reasons: LoadReasons,
    },
    /// Chunk is loaded under multiple loadshares.
    Many(LoadshareMap<LoadReasons>),
}

/// The result of removing a loadshare from [`ChunkLoadshares`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LoadshareRemovalResult {
    /// The load reasons were updated but did not turn out empty.
    LoadReasonsUpdated,
    /// The specified loadshare had no remaining load reasons after the removal and was removed from this chunk.
    LoadshareRemoved,
    /// The chunk has no loadshares and/or this loadshare was not present for this chunk.
    NoLoadshare,
}

impl ChunkLoadshares {
    pub fn single(loadshare: LoadshareId, reasons: LoadReasons) -> Self {
        Self(ChunkLoadsharesInner::Single { loadshare, reasons })
    }

    /// Returns `true` if there are no loadshares (and therefore no load reasons) for this chunk.
    #[inline]
    pub fn is_empty(&self) -> bool {
        matches!(self.0, ChunkLoadsharesInner::Empty)
    }

    /// Iterate through all the loadshares.
    #[inline]
    pub fn iter_loadshares(&self) -> impl Iterator<Item = LoadshareId> {
        match &self.0 {
            ChunkLoadsharesInner::Empty => Vec::new().into_iter(),
            ChunkLoadsharesInner::Single { loadshare, .. } => {
                Vec::from_iter([*loadshare]).into_iter()
            }
            ChunkLoadsharesInner::Many(map) => map.keys().cloned().collect_vec().into_iter(),
        }
    }

    /// Get the union of all the load reasons.
    #[inline]
    pub fn load_reasons_union(&self) -> LoadReasons {
        match &self.0 {
            ChunkLoadsharesInner::Empty => LoadReasons::empty(),
            ChunkLoadsharesInner::Single { reasons, .. } => reasons.clone(),
            ChunkLoadsharesInner::Many(loadshares) => loadshares
                .values()
                .cloned()
                .reduce(|acc, reasons| acc.r#union(reasons))
                .unwrap_or(LoadReasons::empty()),
        }
    }

    /// Insert some new load reasons for a loadshare. Will update the existing load reasons if the loadshare
    /// is already present.
    #[inline]
    pub fn insert(&mut self, new_loadshare: LoadshareId, new_reasons: LoadReasons) {
        match &mut self.0 {
            // If we were empty, we re-initialize ourselves to a single loadshare.
            ChunkLoadsharesInner::Empty => *self = Self::single(new_loadshare, new_reasons),
            ChunkLoadsharesInner::Single { loadshare, reasons } => {
                if *loadshare == new_loadshare {
                    // The new loadshare is the same as the existing, single loadshare, so we just update
                    // the single load reasons.
                    reasons.insert(new_reasons);
                } else {
                    // We have more than 2 loadshares so we need to move to the heap.
                    // Also keep our existing loadshare and its reasons.
                    self.0 = ChunkLoadsharesInner::Many(HashMap::from_iter([
                        (*loadshare, *reasons),
                        (new_loadshare, new_reasons),
                    ]));
                }
            }
            ChunkLoadsharesInner::Many(loadshares) => {
                // Update existing reasons or insert this loadshare as a new one.
                loadshares
                    .entry(new_loadshare)
                    .and_modify(|reasons| reasons.insert(new_reasons))
                    .or_insert(new_reasons);
            }
        }
    }

    /// Remove load reasons from a loadshare. If the loadshare ends up having no reasons left it will be removed
    /// from the [`ChunkLoadshares`]. If the removed loadshare was the only loadshare then the [`ChunkLoadshares`]
    /// will turn into [`ChunkLoadshares::Empty`], in which case the chunk should be purged.
    #[inline]
    pub fn remove(
        &mut self,
        remove_loadshare: LoadshareId,
        remove_reasons: LoadReasons,
    ) -> LoadshareRemovalResult {
        match &mut self.0 {
            // We're already empty so there's nothing to do.
            ChunkLoadsharesInner::Empty => return LoadshareRemovalResult::NoLoadshare,
            ChunkLoadsharesInner::Single { loadshare, reasons } => {
                // The loadshare was not present for this chunk,
                // so there's nothing to remove and we can return early.
                if *loadshare != remove_loadshare {
                    return LoadshareRemovalResult::NoLoadshare;
                }

                reasons.remove(remove_reasons);
                // Since this chunk was only loaded under a single loadshare, and we removed all the reasons for
                // that loadshare, we no longer have any reason to be loaded and we are empty.
                if reasons.is_empty() {
                    self.0 = ChunkLoadsharesInner::Empty;
                    LoadshareRemovalResult::LoadshareRemoved
                } else {
                    LoadshareRemovalResult::LoadReasonsUpdated
                }
            }
            ChunkLoadsharesInner::Many(loadshares) => {
                let Entry::Occupied(mut entry) = loadshares.entry(remove_loadshare) else {
                    // As usual, return early if the loadshare doesn't exist for this chunk.
                    return LoadshareRemovalResult::NoLoadshare;
                };

                let reasons = entry.get_mut();
                reasons.remove(remove_reasons);
                if reasons.is_empty() {
                    // If there are no remaining load reasons, we can remoe this loadshare from the chunk.
                    entry.remove();

                    // We should never end up in a situation where the loadshare hashmap is empty. If there
                    // are no remaining loadshares for this chunk then we should be a Self::Empty variant.
                    debug_assert!(!loadshares.is_empty());
                    if loadshares.len() == 1 {
                        let (&loadshare, &reasons) = loadshares.iter().next().unwrap();
                        *self = Self::single(loadshare, reasons);
                    }

                    LoadshareRemovalResult::LoadshareRemoved
                } else {
                    LoadshareRemovalResult::LoadReasonsUpdated
                }
            }
        }
    }
}

/// Table of loadshare ownership of chunks. Mainly used to organize the interests of different loaders so that
/// chunks are only unloaded when there is consensus among loaders to do so.
#[derive(Default)]
pub struct ChunkLoadshareTable {
    /// A map of all chunks and the loadshares they are loaded under with the reasons why.
    loadshares_for_chunks: DashMap<ChunkPos, ChunkLoadshares, ChunkStorageHasher>,
    /// A map of all loadshares and the chunks they have loaded (for any reason).
    chunks_for_loadshares: DashMap<LoadshareId, ChunkSet, LoadshareIdHasher>,
}
