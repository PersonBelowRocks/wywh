use super::chunk::{Chunk, ChunkFlags, ChunkPos, ChunkReadHandle, ChunkWriteHandle};
use crate::util::sync::{LockStrategy, StrategicReadLock, StrategicWriteLock, StrategySyncError};
use flume::Sender;
use std::ops::Deref;
use std::sync::Arc;

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
    pub(super) chunk: Arc<Chunk>,
    pub(super) notify_changed: &'a Sender<ChunkPos>,
}

impl<'a> ChunkRef<'a> {
    /// Get the position of this chunk.
    #[inline]
    pub fn chunk_pos(&self) -> ChunkPos {
        self.chunk.chunk_pos()
    }

    /// Get the underlying chunk for this reference. To write to or read from a chunk you'll need
    /// to do operations on this underlying chunk.
    #[inline]
    pub fn chunk(&self) -> &Chunk {
        self.chunk.deref()
    }

    /// Get the flags for this chunk, locking according to the given lock strategy.
    #[inline]
    pub fn flags(&self, strategy: LockStrategy) -> Result<ChunkFlags, StrategySyncError> {
        self.chunk()
            .flags
            .strategic_read(strategy)
            .map(|flags| flags.clone())
    }

    /// Check if this chunk is primordial.
    ///
    /// Internally this locks the chunk flags blockingly, so be wary of performance.
    #[inline]
    pub fn is_primordial(&self) -> bool {
        self.chunk()
            .flags
            .strategic_read(LockStrategy::Blocking)
            .unwrap()
            .contains(ChunkFlags::PRIMORDIAL)
    }

    /// Set the flags of this chunk. You should usually always prefer [`Chunk::update_flags`] over
    /// this function as this function completely overwrites the existing flags.
    #[inline]
    pub fn set_flags(
        &self,
        strategy: LockStrategy,
        new_flags: ChunkFlags,
    ) -> Result<(), StrategySyncError> {
        let mut old_flags = self.chunk().flags.strategic_write(strategy)?;

        *old_flags = new_flags;

        Ok(())
    }

    /// Calls the closure with a mutable reference to the existing flags, allowing the caller
    /// to make changes to specific flags while leaving others untouched.
    #[inline]
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

    /// Get a read handle for this chunk with the given [lock strategy].
    /// The returned error depends on the lock strategy, see [`StrategySyncError`] for more information.
    ///
    /// [lock strategy]: LockStrategy
    pub fn read_handle(
        &self,
        strategy: LockStrategy,
    ) -> Result<ChunkReadHandle<'_>, StrategySyncError> {
        let chunk = self.chunk();

        Ok(ChunkReadHandle {
            blocks: chunk.blocks.strategic_read(strategy)?,
        })
    }

    /// Get a write handle for this chunk with the given [lock strategy].
    /// The returned error depends on the lock strategy, see [`StrategySyncError`] for more information.
    ///
    /// [lock strategy]: LockStrategy
    pub fn write_handle(
        &self,
        strategy: LockStrategy,
    ) -> Result<ChunkWriteHandle<'_>, StrategySyncError> {
        let chunk = self.chunk();

        Ok(ChunkWriteHandle {
            blocks: chunk.blocks.strategic_write(strategy)?,
            flags: chunk.flags.strategic_write(strategy)?,
            chunk_pos: self.chunk_pos(),
            notify: &self.notify_changed,
        })
    }
}
