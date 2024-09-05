use std::sync::Arc;

use dashmap::DashMap;

use crate::topo::{
    controller::LoadReasons,
    world::{Chunk, ChunkPos},
};

/// The hash function used for chunk storage
pub type ChunkStorageHasher = rustc_hash::FxBuildHasher;

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
