use bevy::{ecs::entity::Entity, math::UVec3, prelude::IVec3};
use parking_lot::RwLockReadGuard;
use std::hash::BuildHasher;
use std::ops::Deref;

use super::{
    chunk::{Chunk, ChunkFlags, ChunkPos},
    chunk_manager::{ChunkStatuses, LccRef},
    ChunkManagerError,
};
use crate::{
    topo::{
        block::{BlockVoxel, FullBlock, Microblock, SubdividedBlock},
        controller::LoadReasons,
    },
    util::sync::{LockStrategy, StrategicReadLock, StrategicWriteLock, StrategySyncError},
};

macro_rules! update_status_for_flag {
    ($field:expr, $chunk_pos:expr, $new_flags:expr, $flag:expr) => {
        if $new_flags.contains($flag) {
            $field.insert($chunk_pos);
        } else {
            $field.remove(&$chunk_pos);
        }
    };
}

/// A reference to a chunk. This type internally includes some additional metadata for chunks
/// that individual chunks don't have. Since this type is provided by the chunk manager, the
/// chunk manager can attach some data to the references it returns like a set of all updated chunks or
/// the entity this chunk is tied to (if it exists). Updating the flags of a chunk reference will
/// also update the map of all updated chunks accordingly.
pub struct ChunkRef<'a> {
    pub(super) chunk: LccRef<'a>,
    pub(super) stats: RwLockReadGuard<'a, ChunkStatuses>,
    pub(super) entity: Option<Entity>,
}

impl<'a> ChunkRef<'a> {
    /// Get the position of this chunk.
    pub fn chunk_pos(&self) -> ChunkPos {
        self.chunk.chunk_pos()
    }

    /// Get the entity associated with the chunk if there is one.
    pub fn entity(&self) -> Option<Entity> {
        self.entity
    }

    /// Get the underlying chunk for this reference. To write to or read from a chunk you'll need
    /// to do operations on this underlying chunk.
    pub fn chunk(&self) -> &Chunk {
        self.chunk.deref()
    }

    /// Get the flags for this chunk, locking according to the given lock strategy.
    pub fn flags(&self, strategy: LockStrategy) -> Result<ChunkFlags, StrategySyncError> {
        self.chunk()
            .flags
            .strategic_read(strategy)
            .map(|flags| flags.clone())
    }

    /// Set the flags of this chunk. You should usually always prefer [`Chunk::update_flags`] over
    /// this function as this function completely overwrites the existing flags.
    pub fn set_flags(
        &self,
        strategy: LockStrategy,
        new_flags: ChunkFlags,
    ) -> Result<(), StrategySyncError> {
        let mut old_flags = self.chunk().flags.strategic_write(strategy)?;

        *old_flags = new_flags;

        update_status_for_flag!(
            self.stats.remesh,
            self.chunk_pos(),
            new_flags,
            ChunkFlags::REMESH
        );

        update_status_for_flag!(
            self.stats.solid,
            self.chunk_pos(),
            new_flags,
            ChunkFlags::SOLID
        );

        Ok(())
    }

    /// Calls the closure with a mutable reference to the existing flags, allowing the caller
    /// to make changes to specific flags while leaving others untouched.
    pub fn update_flags<F>(&self, strategy: LockStrategy, f: F) -> Result<(), StrategySyncError>
    where
        F: for<'flags> FnOnce(&'flags mut ChunkFlags),
    {
        let old_flags = self.flags(strategy)?;
        let mut new_flags = old_flags;
        f(&mut new_flags);

        self.set_flags(strategy, new_flags)?;
        Ok(())
    }

    /// A union of all loadshares' reasons for having this chunk loaded.
    pub fn cached_load_reasons(&self) -> LoadReasons {
        self.chunk().load_reasons.read().cached_reasons
    }
}
