use std::sync::Arc;

use hb::HashMap;
use parking_lot::RwLock;

use crate::topo::{
    controller::LoadReasons,
    world::{new_chunk_manager::chunk_pos_in_bounds, Chunk, ChunkPos},
};

use super::error::{ChunkLoadError, ChunkPurgeError};

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
    loaded: RwLock<HashMap<ChunkPos, LoadedChunk, ChunkStorageHasher>>,
    /// A temporary stop for chunks before they are unloaded. Chunks in purgatory should not
    /// be modified, but they may be revived and moved back to the loaded state.
    purgatory: RwLock<HashMap<ChunkPos, Arc<Chunk>, ChunkStorageHasher>>,
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

    /// Add the given load reasons to a loaded chunks load reasons, updating them. Does nothing if
    /// the chunk is not loaded.
    #[inline]
    pub fn add_load_reasons(&self, chunk_pos: ChunkPos, reasons: LoadReasons) {
        self.loaded
            .write()
            .entry(chunk_pos)
            .and_modify(|loaded_chunk| loaded_chunk.load_reasons.insert(reasons));
    }

    /// Load a chunk. This function will attempt to revive a chunk from purgatory if possible.
    /// For this reason the provided chunk may not necessarily be the one that gets loaded.
    ///
    /// A returned error of [`ChunkLoadError::AlreadyLoaded`] is not necessarily a problem, but rather
    /// a warning to the caller that their call might not have had the expected effect.
    ///
    /// Only loads the provided chunk if:
    /// - Chunk at this position is not already loaded.
    /// - Chunk at this position is not in purgatory.
    ///
    /// Revives chunk from purgatory if:
    /// - Chunk at this position is not already loaded.
    /// - Chunk at this position exists in purgatory.
    ///
    /// # Panics
    /// Will panic if the provided chunk is both loaded and in purgatory (should NEVER happen).
    #[inline]
    pub fn load(&self, chunk: Chunk, reasons: LoadReasons) -> Result<(), ChunkLoadError> {
        let chunk_pos = chunk.chunk_pos();
        if !chunk_pos_in_bounds(chunk_pos) {
            return Err(ChunkLoadError::out_of_bounds(chunk_pos));
        }

        if reasons.is_empty() {
            return Err(ChunkLoadError::NoReasons(chunk_pos));
        }

        let mut loaded = self.loaded.write();
        let mut purgatory = self.purgatory.write();

        match purgatory.remove(&chunk_pos) {
            // Revive chunk from purgatory wherever possible to re-use resources.
            Some(revived) => {
                // Make sure the chunk positions are actually the same. If they're not something
                // has gone seriously wrong.
                debug_assert_eq!(revived.chunk_pos(), chunk_pos);

                // Chunks must NEVER be both loaded and in purgatory.
                if loaded.contains_key(&chunk_pos) {
                    // Re-insert the chunk into purgatory so we don't leave the storage in a broken state.
                    purgatory.insert(chunk_pos, revived);
                    panic!("Chunk {chunk_pos} was in purgatory, but was also loaded");
                }

                loaded.insert(
                    chunk_pos,
                    LoadedChunk {
                        chunk: revived,
                        load_reasons: reasons,
                    },
                );
            }
            None => {
                // We only insert this chunk is it doesn't already exist. Already alive chunks take
                // priority over revived chunks.
                if loaded.contains_key(&chunk_pos) {
                    return Err(ChunkLoadError::AlreadyLoaded(chunk_pos));
                } else {
                    // Just to be 100% sure we do this instead of a regular .insert() call.
                    loaded.entry(chunk_pos).or_insert(LoadedChunk {
                        load_reasons: reasons,
                        chunk: Arc::new(chunk),
                    });
                }
            }
        }

        // If we reach this point the chunk should be loaded and not in purgatory.
        debug_assert!(loaded.contains_key(&chunk_pos));
        debug_assert!(!purgatory.contains_key(&chunk_pos));

        // Explicitly drop our RW lock guards for clarity.
        drop(loaded);
        drop(purgatory);

        Ok(())
    }

    /// Purge this chunk, moving it into purgatory to be cleaned up and unloaded.
    ///
    /// Returns an error if:
    /// - Chunk was not loaded to begin with.
    /// - Chunk was already in purgatory.
    /// - Chunk was out of bounds.
    #[inline]
    pub fn purge(&self, chunk_pos: ChunkPos) -> Result<(), ChunkPurgeError> {
        if !chunk_pos_in_bounds(chunk_pos) {
            return Err(ChunkPurgeError::out_of_bounds(chunk_pos));
        }

        let mut loaded = self.loaded.write();
        let mut purgatory = self.purgatory.write();

        if purgatory.contains_key(&chunk_pos) {
            return Err(ChunkPurgeError::AlreadyPurged(chunk_pos));
        }

        if !loaded.contains_key(&chunk_pos) {
            return Err(ChunkPurgeError::NotLoaded(chunk_pos));
        }

        let purged_chunk = loaded
            .remove(&chunk_pos)
            .expect("we just checked that this key exists")
            .chunk;
        // Make sure that the positions match up.
        debug_assert_eq!(chunk_pos, purged_chunk.chunk_pos());

        purgatory.insert(chunk_pos, purged_chunk);

        // Explicitly drop our RW lock guards for clarity.
        drop(loaded);
        drop(purgatory);

        Ok(())
    }
}
