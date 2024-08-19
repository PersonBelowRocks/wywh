use std::hash::BuildHasher;

use bevy::{ecs::entity::Entity, math::UVec3, prelude::IVec3};
use parking_lot::RwLockReadGuard;

use crate::topo::{
    block::{BlockVoxel, FullBlock, Microblock, SubdividedBlock},
    controller::LoadReasons,
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
}
