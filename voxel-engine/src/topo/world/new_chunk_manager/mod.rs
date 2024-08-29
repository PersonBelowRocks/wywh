use std::ops::Range;

use dashmap::DashSet;
use error::{ChunkGetError, ChunkLoadError};
use hb::{hash_map::Entry, HashMap};
use inner_storage::{ChunkStorageHasher, InnerChunkStorage};
use itertools::Itertools;
use parking_lot::RwLock;

use crate::{
    data::registries::block::BlockVariantId,
    topo::controller::{LoadReasons, LoadshareId, LoadshareMap},
    util::ChunkSet,
};

use super::{chunk::ChunkFlags, Chunk, ChunkPos, ChunkRef};

mod ecs;
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
pub struct ChunkManager2 {
    pub(super) default_block: BlockVariantId,
    pub(super) storage: InnerChunkStorage,
    pub(super) statuses: ChunkStatuses,
    pub(super) loadshares: ChunkLoadshareTable,
}

impl ChunkManager2 {
    /// Create a new chunk manager with a default block.
    pub fn new(default_block: BlockVariantId) -> Self {
        Self {
            default_block,
            storage: InnerChunkStorage::default(),
            statuses: ChunkStatuses::default(),
            loadshares: ChunkLoadshareTable::default(),
        }
    }

    /// Whether the given chunk is loaded or not.
    #[inline]
    pub fn is_loaded(&self, chunk_pos: ChunkPos) -> bool {
        self.storage.is_loaded(chunk_pos)
    }

    /// Get the chunk loaded at the given position.
    #[inline]
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
            stats: todo!(), // TODO: &self.statuses,
            entity: None,
        })
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
    pub(super) loadshares_for_chunks:
        RwLock<HashMap<ChunkPos, ChunkLoadshares, ChunkStorageHasher>>,
    /// A map of all loadshares and the chunks they have loaded (for any reason).
    pub(super) chunks_for_loadshares: RwLock<LoadshareMap<ChunkSet>>,
}
