use std::sync::Arc;

use cb::channel::Sender;
use hb::{hash_map::Entry, HashMap};
use parking_lot::RwLock;

use bevy::log::error;

use crate::topo::{
    controller::{LoadChunks, LoadReasons, UnloadChunks},
    world::{chunk::ChunkFlags, new_chunk_manager::chunk_pos_in_bounds, Chunk, ChunkPos},
    worldgen::{generator::GenerateChunk, GenerationPriority},
};

use super::{
    error::{ChunkLoadError, ChunkPurgeError},
    ChunkManager2,
};

/// The hash function used for chunk storage
type ChunkStorageHasher = fxhash::FxBuildHasher;

#[derive(Clone)]
pub struct LoadedChunk {
    pub chunk: Arc<Chunk>,
    pub load_reasons: LoadReasons,
}

/// The inner chunk storage of the chunk manager. This is where chunks "live".
#[derive(Default)]
pub struct InnerChunkStorage {
    /// All loaded chunks, wrapped in an RW lock for thread safety.
    pub(super) loaded: RwLock<HashMap<ChunkPos, LoadedChunk, ChunkStorageHasher>>,
    /// A temporary stop for chunks before they are unloaded. Chunks in purgatory should not
    /// be modified, but they may be revived and moved back to the loaded state.
    pub(super) purgatory: RwLock<HashMap<ChunkPos, Arc<Chunk>, ChunkStorageHasher>>,
}

static_assertions::assert_impl_all!(InnerChunkStorage: Send, Sync);

impl InnerChunkStorage {
    /// Get a strong reference to the chunk at the given position, if it's loaded.
    /// Chunks in purgatory are not loaded and therefore will not be accessible through this function.
    #[inline]
    pub fn get(&self, chunk_pos: ChunkPos) -> Option<Arc<Chunk>> {
        self.loaded.read().get(&chunk_pos).map(|e| e.chunk.clone())
    }

    /// Whether the given chunk is loaded in this storage.
    #[inline]
    pub fn is_loaded(&self, chunk_pos: ChunkPos) -> bool {
        self.loaded.read().contains_key(&chunk_pos)
    }
}

/// The internal chunk load task. Should be run in an async task to avoid blocking the whole app.
#[inline]
pub(super) fn load_chunks_from_event(
    cm: &ChunkManager2,
    mut event: LoadChunks,
    generation_events: &Sender<GenerateChunk>,
    lock_granularity: usize,
) {
    while !event.chunks.is_empty() {
        // We're coarsely locking here to give other tasks a chance to make changes to the chunk storage.
        let mut loaded = cm.storage.loaded.write();
        let mut purgatory = cm.storage.purgatory.write();

        for _ in 0..lock_granularity {
            let Some(chunk_pos) = event.chunks.pop() else {
                break;
            };

            let mut should_generate = event.auto_generate;

            loaded
                .entry(chunk_pos)
                .and_modify(|loaded_chunk| {
                    // Modify the load reasons if the chunk is already loaded.
                    loaded_chunk.load_reasons.insert(event.reasons);
                    // Don't send generation events if the chunk is already loaded.
                    should_generate = false;
                })
                .or_insert_with(|| LoadedChunk {
                    load_reasons: event.reasons,
                    // If the chunk we're trying to load is in purgatory, then resurrect it instead of creating a new one.
                    chunk: purgatory
                        .remove(&chunk_pos)
                        .unwrap_or_else(|| Arc::new(cm.new_primordial_chunk(chunk_pos))),
                });

            if should_generate {
                generation_events
                    .send(GenerateChunk {
                        chunk_pos,
                        priority: GenerationPriority::new(0),
                    })
                    .unwrap();
            }
        }

        // Explicitly drop our RW lock guards for clarity.
        drop(loaded);
        drop(purgatory);
    }
}

/// The internal chunk purge task. Should be run in an async task to avoid blocking the whole app.
#[inline]
pub(super) fn purge_chunks_from_event(
    cm: &ChunkManager2,
    mut event: UnloadChunks,
    lock_granularity: usize,
) {
    while !event.chunks.is_empty() {
        // We're coarsely locking here to give other tasks a chance to make changes to the chunk storage.
        let mut loaded = cm.storage.loaded.write();
        let mut purgatory = cm.storage.purgatory.write();

        for _ in 0..lock_granularity {
            let Some(chunk_pos) = event.chunks.pop() else {
                break;
            };

            // Get the occupied entry for this chunk.
            let Entry::Occupied(mut entry) = loaded.entry(chunk_pos) else {
                continue;
            };

            // This case should never happen and would be a bug, so we need to catch this error and abort.
            if purgatory.contains_key(&chunk_pos) {
                panic!("Loaded chunk was found in purgatory: {chunk_pos}");
            }

            let loaded_chunk = entry.get_mut();

            loaded_chunk.load_reasons.remove(event.reasons);
            // No remaining load reasons so we can actually move this chunk to purgatory.
            if loaded_chunk.load_reasons.is_empty() {
                let chunk = entry.remove();
                purgatory.insert(chunk_pos, chunk.chunk);
            }
        }

        // Explicitly drop our RW lock guards for clarity.
        drop(loaded);
        drop(purgatory);
    }
}
