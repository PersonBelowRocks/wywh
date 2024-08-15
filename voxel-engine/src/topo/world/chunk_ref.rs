use std::hash::BuildHasher;

use bevy::{ecs::entity::Entity, math::UVec3, prelude::IVec3};
use parking_lot::RwLockReadGuard;

use crate::topo::{
    block::{BlockVoxel, FullBlock, Microblock, SubdividedBlock},
    controller::LoadReasons,
    error::ChunkAccessError,
};

use super::{
    chunk::{Chunk, ChunkFlags, ChunkPos},
    chunk_manager::{ChunkStatuses, LccRef},
    ChunkManagerError,
};

pub struct ChunkRef<'a> {
    pub(super) chunk: LccRef<'a>,
    pub(super) stats: RwLockReadGuard<'a, ChunkStatuses>,
    pub(super) pos: ChunkPos,
    pub(super) entity: Option<Entity>,
}

impl<'a> ChunkRef<'a> {
    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn entity(&self) -> Option<Entity> {
        self.entity
    }

    pub fn flags(&self) -> ChunkFlags {
        *self.chunk.flags.read()
    }

    fn set_flags(&self, new_flags: ChunkFlags) {
        let mut old_flags = self.chunk.flags.write();
        *old_flags = new_flags
    }

    pub fn update_flags<F>(&self, f: F)
    where
        F: for<'flags> FnOnce(&'flags mut ChunkFlags),
    {
        let old_flags = self.flags();
        let mut new_flags = old_flags;
        f(&mut new_flags);

        if new_flags.contains(ChunkFlags::FRESHLY_GENERATED) {
            self.stats.fresh.insert(self.pos);
        } else {
            self.stats.fresh.remove(&self.pos);
        }

        if new_flags.contains(ChunkFlags::GENERATING) {
            self.stats.generating.insert(self.pos);
        } else {
            self.stats.generating.remove(&self.pos);
        }

        if new_flags.contains(ChunkFlags::REMESH) {
            self.stats.updated.insert(self.pos);
        } else {
            self.stats.updated.remove(&self.pos);
        }

        self.set_flags(new_flags);
    }

    pub fn cached_load_reasons(&self) -> LoadReasons {
        self.chunk.load_reasons.read().cached_reasons
    }
}
