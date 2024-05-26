use std::sync::Arc;

use bevy::{
    ecs::entity::Entity,
    math::{ivec3, IVec3},
};
use dashmap::{mapref::one::Ref, DashSet};

use crate::{
    topo::{
        block::{BlockVoxel, FullBlock},
        controller::LoadReasons,
        neighbors::{Neighbors, NEIGHBOR_ARRAY_SIZE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS},
    },
    util::{ivec3_to_1d, ChunkSet, SyncHashMap},
};

use super::{chunk::ChunkFlags, Chunk, ChunkManagerError, ChunkPos, ChunkRef, ChunkRefReadAccess};

#[derive(Default)]
pub struct LoadedChunkContainer(pub(crate) SyncHashMap<ChunkPos, Chunk>);

pub type LccRef<'a> = Ref<'a, ChunkPos, Chunk, ahash::RandomState>;

pub struct LoadedChunkIterator<'a>(pub(crate) hb::hash_map::Iter<'a, ChunkPos, Chunk>);

pub type StrongChunkRef<'a> =
    dashmap::mapref::one::Ref<'a, ChunkPos, Arc<Chunk>, ahash::RandomState>;

impl LoadedChunkContainer {
    pub fn get(&self, pos: ChunkPos) -> Option<LccRef<'_>> {
        self.0.get(&pos)
    }

    pub fn set(&self, pos: ChunkPos, chunk: Chunk) {
        self.0.insert(pos, chunk);
    }

    pub fn remove(&self, pos: ChunkPos) -> bool {
        self.0.remove(&pos).is_some()
    }

    pub(crate) fn internal_map(&self) -> &'_ SyncHashMap<ChunkPos, Chunk> {
        &self.0
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
