use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

use bevy::{math::IVec3, prelude::Resource};

use crate::{
    data::tile::Face,
    render::meshing::Neighbors,
    util::{FaceMap, SyncHashMap},
};

use super::{
    access::{ChunkBounds, ReadAccess},
    chunk::{Chunk, ChunkPos},
    chunk_ref::{ChunkRef, ChunkRefVxlReadAccess, ChunkVoxelOutput},
    error::ChunkManagerGetChunkError,
};

#[derive(Default)]
pub struct LoadedChunkContainer(pub(crate) SyncHashMap<ChunkPos, Arc<Chunk>>);

pub struct LoadedChunkIterator<'a>(pub(crate) hb::hash_map::Iter<'a, ChunkPos, Arc<Chunk>>);

pub type StrongChunkRef<'a> =
    dashmap::mapref::one::Ref<'a, ChunkPos, Arc<Chunk>, ahash::RandomState>;

impl LoadedChunkContainer {
    pub fn get(&self, pos: ChunkPos) -> Option<Weak<Chunk>> {
        self.0.get(&pos).as_deref().map(Arc::downgrade)
    }

    pub fn get_strong(&self, pos: ChunkPos) -> Option<StrongChunkRef<'_>> {
        self.0.get(&pos)
    }

    pub fn set(&self, pos: ChunkPos, chunk: Arc<Chunk>) {
        self.0.insert(pos, chunk);
    }

    pub fn remove(&self, pos: ChunkPos) -> bool {
        self.0.remove(&pos).is_some()
    }

    pub(crate) fn internal_map(&self) -> &'_ SyncHashMap<ChunkPos, Arc<Chunk>> {
        &self.0
    }
}

#[derive(Default)]
pub struct PendingChunkChanges(pub(crate) SyncHashMap<ChunkPos, Arc<AtomicBool>>);

impl PendingChunkChanges {
    pub fn get(&self, pos: ChunkPos) -> Option<Weak<AtomicBool>> {
        self.0.get(&pos).as_deref().map(Arc::downgrade)
    }

    pub fn get_or_create(&self, pos: ChunkPos, initial_status: bool) -> Weak<AtomicBool> {
        self.get(pos)
            .unwrap_or_else(|| self.create(pos, initial_status))
    }

    pub fn update_or_create(&self, pos: ChunkPos, changed: bool) -> Weak<AtomicBool> {
        let weak = self.get_or_create(pos, changed);

        if let Some(b) = weak.upgrade() {
            b.store(changed, Ordering::SeqCst)
        }

        weak
    }

    pub fn create(&self, pos: ChunkPos, initial_status: bool) -> Weak<AtomicBool> {
        let atomic_bool = Arc::new(AtomicBool::new(initial_status));
        let weak = Arc::downgrade(&atomic_bool);

        self.0.insert(pos, atomic_bool);

        weak
    }

    pub fn remove(&self, pos: ChunkPos) -> bool {
        self.0.remove(&pos).is_some()
    }

    pub(crate) fn internal_map(&self) -> &'_ SyncHashMap<ChunkPos, Arc<AtomicBool>> {
        &self.0
    }

    pub(crate) fn pending_changes(&self) -> Vec<ChunkPos> {
        self.0
            .iter()
            .filter(|m| m.value().swap(false, Ordering::SeqCst))
            .map(|m| *m.key())
            .collect::<Vec<_>>()
    }
}

pub(crate) struct ChangedChunks<'a, 'b> {
    changed_positions: Vec<ChunkPos>,
    changes: &'a SyncHashMap<ChunkPos, Arc<AtomicBool>>,
    chunks: &'b SyncHashMap<ChunkPos, Arc<Chunk>>,
}

impl<'a, 'b> Iterator for ChangedChunks<'a, 'b> {
    type Item = ChunkRef;

    fn next(&mut self) -> Option<Self::Item> {
        const ERROR_MSG: &str = "All the positions in this iterator should be ensured to lead to actual loaded chunks because the state of the realm should be frozen when this iterator is obtained";

        let pos = self.changed_positions.pop()?;

        Some(ChunkRef {
            pos,
            chunk: Arc::downgrade(self.chunks.get(&pos).as_deref().expect(ERROR_MSG)),
            changed: Arc::downgrade(self.changes.get(&pos).as_deref().expect(ERROR_MSG)),
        })
    }
}

#[derive(Default)]
pub struct ChunkManager {
    loaded_chunks: LoadedChunkContainer,
    pending_changes: PendingChunkChanges,
}

impl ChunkManager {
    pub fn new() -> Self {
        Self {
            loaded_chunks: LoadedChunkContainer::default(),
            pending_changes: PendingChunkChanges::default(),
        }
    }

    pub fn get_loaded_chunk(&self, pos: ChunkPos) -> Result<ChunkRef, ChunkManagerGetChunkError> {
        let chunk = self
            .loaded_chunks
            .get(pos)
            .ok_or(ChunkManagerGetChunkError::Unloaded)?;
        let changed = self
            .pending_changes
            .get(pos)
            .ok_or(ChunkManagerGetChunkError::Unloaded)?;

        Ok(ChunkRef {
            chunk,
            changed,
            pos,
        })
    }

    pub(crate) fn get_chunk(
        &self,
        pos: ChunkPos,
    ) -> Result<StrongChunkRef<'_>, ChunkManagerGetChunkError> {
        self.loaded_chunks
            .get_strong(pos)
            .ok_or(ChunkManagerGetChunkError::Unloaded)
    }

    // TODO: test
    pub fn with_neighbors<A, F, R>(
        &self,
        pos: ChunkPos,
        mut f: F,
    ) -> Result<R, ChunkManagerGetChunkError>
    where
        A: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds,
        F: for<'a> FnMut(Neighbors<ChunkRefVxlReadAccess<'a, ahash::RandomState>>) -> R,
    {
        // we need to make a map of the neighboring chunks so that the references are owned by the function scope
        let chunks = FaceMap::from_fn(|face| {
            let nb_pos = ChunkPos::from(IVec3::from(pos) + face.normal());
            self.get_chunk(nb_pos).ok()
        });

        let mut map = FaceMap::<ChunkRefVxlReadAccess>::new();

        for face in Face::FACES {
            let Some(cref) = chunks.get(face) else {
                continue;
            };

            let access = ChunkRefVxlReadAccess {
                transparency: cref.transparency.read_access(),
                variants: cref.variants.read_access(),
            };

            map.set(face, access);
        }

        todo!()
    }

    pub fn set_loaded_chunk(&self, pos: ChunkPos, chunk: Chunk) {
        self.loaded_chunks.set(pos, Arc::new(chunk));
        self.pending_changes.update_or_create(pos, true);
    }

    pub(crate) fn changed_chunks(&self) -> ChangedChunks<'_, '_> {
        ChangedChunks {
            changed_positions: self.pending_changes.pending_changes(),
            changes: self.pending_changes.internal_map(),
            chunks: self.loaded_chunks.internal_map(),
        }
    }
}

#[derive(Resource)]
pub struct VoxelRealm {
    pub chunk_manager: ChunkManager,
}

impl VoxelRealm {
    pub fn new() -> Self {
        Self {
            chunk_manager: ChunkManager::new(),
        }
    }
}
