use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use bevy::{
    math::{ivec3, IVec3},
    prelude::Resource,
};
use dashmap::{mapref::one::Ref, DashSet};

use crate::{
    topo::{
        block::{BlockVoxel, FullBlock},
        neighbors::{Neighbors, NEIGHBOR_ARRAY_SIZE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS},
        world::chunk::ChunkFlags,
    },
    util::{ivec3_to_1d, SyncHashMap},
};

use super::{
    chunk::{Chunk, ChunkPos},
    chunk_ref::{ChunkRef, ChunkRefReadAccess},
    error::ChunkManagerError,
};

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

    pub fn get_loaded_chunk(&self, pos: ChunkPos) -> Result<ChunkRef<'_>, ChunkManagerError> {
        let chunk = self
            .loaded_chunks
            .get(pos)
            .ok_or(ChunkManagerError::Unloaded)?;

        Ok(ChunkRef {
            chunk,
            stats: &self.status,
            pos,
        })
    }

    pub fn chunk_flags(&self, pos: ChunkPos) -> Option<ChunkFlags> {
        self.get_loaded_chunk(pos).map(|cref| cref.flags()).ok()
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
                    if let Ok(chunk_ref) = self.get_loaded_chunk(nbrpos_ws) {
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

    pub fn initialize_new_chunk(
        &self,
        pos: ChunkPos,
        generating: bool,
    ) -> Result<ChunkRef, ChunkManagerError> {
        let flags = if generating {
            ChunkFlags::GENERATING
        } else {
            ChunkFlags::empty()
        };

        let chunk = Chunk::new(BlockVoxel::Full(self.default_block), flags);

        if self.loaded_chunks.get(pos).is_some() {
            return Err(ChunkManagerError::AlreadyInitialized);
        }

        self.loaded_chunks.set(pos, chunk);

        self.get_loaded_chunk(pos)
    }

    pub fn updated_chunks(&self) -> UpdatedChunks<'_> {
        UpdatedChunks { manager: &self }
    }
}

pub struct UpdatedChunks<'a> {
    manager: &'a ChunkManager,
}

impl<'a> UpdatedChunks<'a> {
    pub fn num_fresh_chunks(&self) -> usize {
        self.manager.status.fresh.len()
    }

    pub fn num_generating_chunks(&self) -> usize {
        self.manager.status.generating.len()
    }

    pub fn iter_chunks<F>(&self, mut f: F) -> Result<(), ChunkManagerError>
    where
        F: for<'cref> FnMut(ChunkRef<'cref>),
    {
        for chunk_pos in self.manager.status.updated.iter() {
            let cref = self.manager.get_loaded_chunk(*chunk_pos)?;
            f(cref);
        }

        Ok(())
    }
}

#[derive(Resource)]
pub struct VoxelRealm {
    pub chunk_manager: Arc<ChunkManager>,
}

impl VoxelRealm {
    pub fn new(default_block: FullBlock) -> Self {
        Self {
            chunk_manager: Arc::new(ChunkManager::new(default_block)),
        }
    }
}
