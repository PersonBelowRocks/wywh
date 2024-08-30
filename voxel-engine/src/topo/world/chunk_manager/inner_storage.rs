use std::sync::Arc;

use cb::channel::Sender;
use dashmap::{mapref::entry::Entry, DashMap};
use parking_lot::{Mutex, MutexGuard, RwLock};

use bevy::log::error;

use crate::{
    topo::{
        controller::{LoadChunks, LoadReasons, UnloadChunks},
        world::{
            chunk::ChunkFlags,
            chunk_manager::{chunk_pos_in_bounds, LoadshareRemovalResult},
            Chunk, ChunkPos,
        },
        worldgen::{generator::GenerateChunk, GenerationPriority},
    },
    util::{sync::LockStrategy, ChunkSet},
};

use super::{ChunkLoadshares, ChunkManager};

/// The hash function used for chunk storage
pub type ChunkStorageHasher = fxhash::FxBuildHasher;

#[derive(Clone)]
pub struct LoadedChunk {
    pub chunk: Arc<Chunk>,
    /// A cached union of all the different load reasons that loadshares have for this chunk.
    /// Must be recalculated every time a loadshare updates its load reasons for this chunk.
    pub cached_loadshare_reasons_union: LoadReasons,
}

/// The inner chunk storage of the chunk manager. This is where chunks "live".
#[derive(Default)]
pub struct InnerChunkStorage {
    /// All loaded chunks, wrapped in an RW lock for thread safety.
    pub(super) loaded: DashMap<ChunkPos, LoadedChunk, ChunkStorageHasher>,
    /// A temporary stop for chunks before they are unloaded. Chunks in purgatory should not
    /// be modified, but they may be revived and moved back to the loaded state.
    pub(super) purgatory: DashMap<ChunkPos, Arc<Chunk>, ChunkStorageHasher>,
}

static_assertions::assert_impl_all!(InnerChunkStorage: Send, Sync);

impl InnerChunkStorage {
    /// Get a strong reference to the chunk at the given position, if it's loaded.
    /// Chunks in purgatory are not loaded and therefore will not be accessible through this function.
    #[inline]
    pub fn get(&self, chunk_pos: ChunkPos) -> Option<Arc<Chunk>> {
        self.loaded.get(&chunk_pos).map(|e| e.chunk.clone())
    }

    /// Whether the given chunk is loaded in this storage.
    #[inline]
    pub fn is_loaded(&self, chunk_pos: ChunkPos) -> bool {
        self.loaded.contains_key(&chunk_pos)
    }
}

/// The internal chunk load task. Should be run in an async task to avoid blocking the whole app.
#[inline(never)] // never inline so we can easily find this function when debugging
pub(super) fn load_chunks_from_event(
    cm: &ChunkManager,
    mut event: LoadChunks,
    generation_events: &Sender<GenerateChunk>,
    lock_granularity: usize,
) {
    // Don't do anything if there are no reasons. Chunks should never be loaded without any load reasons!
    if event.reasons.is_empty() {
        return;
    }

    while !event.chunks.is_empty() {
        cm.structural_access(LockStrategy::Blocking, |access| {
            // We're coarsely locking here to give other tasks a chance to make changes to the chunk storage.
            for _ in 0..lock_granularity {
                let Some(chunk_pos) = event.chunks.pop() else {
                    break;
                };

                let mut should_generate = event.auto_generate;

                match access.loaded.entry(chunk_pos) {
                    // If the chunk is already loaded, place it under this loadshare and update its load reasons
                    Entry::Occupied(mut occupied_chunk) => {
                        // Don't send generation event if the chunk is already loaded.
                        should_generate = false;

                        let loaded_chunk = occupied_chunk.get_mut();

                        // Add this loadshare to the chunk.
                        let loadshares = access
                            .loadshares_for_chunks
                            .entry(chunk_pos)
                            .and_modify(|loadshares| {
                                loadshares.insert(event.loadshare, event.reasons)
                            })
                            .or_insert(ChunkLoadshares::single(event.loadshare, event.reasons));

                        // Update the cached load reasons.
                        let load_reasons = loadshares.load_reasons_union();
                        loaded_chunk.cached_loadshare_reasons_union = load_reasons;

                        // Add this chunk to the loadshare.
                        access
                            .chunks_for_loadshares
                            .entry(event.loadshare)
                            .and_modify(|chunks| {
                                chunks.set(chunk_pos);
                            })
                            .or_insert_with(|| ChunkSet::single(chunk_pos));
                    }
                    Entry::Vacant(vacant_chunk) => {
                        // If a chunk is not loaded, it also cannot have any load reasons or be under any loadshare.
                        debug_assert!(!access.loadshares_for_chunks.contains_key(&chunk_pos));
                        debug_assert!(!access
                            .chunks_for_loadshares
                            .get(&event.loadshare)
                            .is_some_and(|c| c.contains(chunk_pos)));

                        access.loadshares_for_chunks.insert(
                            chunk_pos,
                            ChunkLoadshares::single(event.loadshare, event.reasons),
                        );

                        access
                            .chunks_for_loadshares
                            .entry(event.loadshare)
                            .and_modify(|chunks| {
                                chunks.set(chunk_pos);
                            })
                            .or_insert_with(|| ChunkSet::single(chunk_pos));

                        // Revive the chunk from purgatory if possible, otherwise create a new one.
                        // FIXME: it seems that reviving a chunk doesn't play nicely with meshes, we
                        //  should probably handle this a bit more elegantly
                        let chunk = access
                            .purgatory
                            .remove(&chunk_pos)
                            .unwrap_or_else(|| {
                                (chunk_pos, Arc::new(cm.new_primordial_chunk(chunk_pos)))
                            })
                            .1;

                        vacant_chunk.insert(LoadedChunk {
                            chunk,
                            // Since this is the first time we're loading this chunk, this event's load reasons will be the initial ones.
                            cached_loadshare_reasons_union: event.reasons,
                        });
                    }
                }

                // TODO: should we also avoid sending generation events if the chunk was revived?
                if should_generate {
                    generation_events
                        .send(GenerateChunk {
                            chunk_pos,
                            priority: GenerationPriority::new(0),
                        })
                        .unwrap();
                }
            }
        })
        .unwrap();
    }
}

/// The internal chunk purge task. Should be run in an async task to avoid blocking the whole app.
#[inline(never)] // never inline so we can easily find this function when debugging
pub(super) fn purge_chunks_from_event(
    cm: &ChunkManager,
    mut event: UnloadChunks,
    lock_granularity: usize,
) {
    while !event.chunks.is_empty() {
        // TODO: remove flags from purged chunks
        cm.structural_access(LockStrategy::Blocking, |access| {
            // We're coarsely locking here to give other tasks a chance to make changes to the chunk storage.
            for _ in 0..lock_granularity {
                let Some(chunk_pos) = event.chunks.pop() else {
                    break;
                };

                // Get the occupied entry for this chunk.
                let Entry::Occupied(mut entry) = access.loaded.entry(chunk_pos) else {
                    continue;
                };

                // This case should never happen and would be a bug, so we need to catch this error and abort.
                debug_assert!(!access.purgatory.contains_key(&chunk_pos));

                // If a chunk is loaded it must be loaded under at least one loadshare.
                let mut loadshares = access.loadshares_for_chunks.get_mut(&chunk_pos).unwrap();

                let result = loadshares.remove(event.loadshare, event.reasons);
                if result == LoadshareRemovalResult::LoadshareRemoved {
                    if let Entry::Occupied(mut entry) =
                        access.chunks_for_loadshares.entry(event.loadshare)
                    {
                        entry.get_mut().remove(chunk_pos);
                        // If there are no more chunks loaded under this loadshare, then just remove the whole loadshare.
                        if entry.get_mut().is_empty() {
                            entry.remove();
                        }
                    }
                }

                // No remaining load reasons so we can actually move this chunk to purgatory.
                if loadshares.is_empty() {
                    // Need to drop this reference into the dashmap, otherwise we'll deadlock when we
                    // try to remove the chunk!
                    drop(loadshares);

                    access.loadshares_for_chunks.remove(&chunk_pos);

                    let chunk = entry.remove();
                    access.purgatory.insert(chunk_pos, chunk.chunk);
                } else {
                    // In this case, there are remaining load reasons so we just update the cached load reasons.
                    let cached_load_reasons = loadshares.load_reasons_union();
                    entry.get_mut().cached_loadshare_reasons_union = cached_load_reasons;
                }
            }
        })
        .unwrap();
    }
}
