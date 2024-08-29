use std::ops::Range;

use dashmap::DashSet;
use error::{ChunkGetError, ChunkLoadError};
use inner_storage::InnerChunkStorage;

use crate::{data::registries::block::BlockVariantId, topo::controller::LoadReasons};

use super::{chunk::ChunkFlags, Chunk, ChunkPos, ChunkRef};

mod ecs;
/// Errors related to chunk management.
pub mod error;
mod inner_storage;

/// The vertical bounds of the world. Chunk positions must have their Y within this range.
pub const WORLD_VERTICAL_DIMENSIONS: Range<i32> = -2048..2048;

/// The horizontal bounds of the world. Chunk positions must have their X and Z within this range.
pub const WORLD_HORIZONTAL_DIMENSIONS: Range<i32> = -65536..65536;

/// Check if a chunk position is in bounds for the world size.
#[inline]
pub fn chunk_pos_in_bounds(chunk_pos: ChunkPos) -> bool {
    let [x, y, z] = chunk_pos.as_ivec3().to_array();

    WORLD_HORIZONTAL_DIMENSIONS.contains(&x)
        && WORLD_HORIZONTAL_DIMENSIONS.contains(&z)
        && WORLD_VERTICAL_DIMENSIONS.contains(&y)
}

/// Sets of loaded chunks with certain properties.
#[derive(Default)]
pub struct ChunkStatuses {
    /// Chunks that need remeshing.
    pub remesh: DashSet<ChunkPos, fxhash::FxBuildHasher>,
    /// Chunks that are completely solid.
    pub solid: DashSet<ChunkPos, fxhash::FxBuildHasher>,
}

/// The chunk manager stores and manages the lifecycle of chunks.
pub struct ChunkManager2 {
    pub(super) default_block: BlockVariantId,
    pub(super) storage: InnerChunkStorage,
    pub(super) statuses: ChunkStatuses,
}

impl ChunkManager2 {
    /// Create a new chunk manager with a default block.
    pub fn new(default_block: BlockVariantId) -> Self {
        Self {
            default_block,
            storage: InnerChunkStorage::default(),
            statuses: ChunkStatuses::default(),
        }
    }

    /// Whether the given chunk is loaded or not.
    #[inline]
    pub fn is_loaded(&self, chunk_pos: ChunkPos) -> bool {
        self.storage.is_loaded(chunk_pos)
    }

    /// Get the chunk loaded at the given position.
    #[inline]
    pub fn loaded_chunk(&self, chunk_pos: ChunkPos) -> Result<ChunkRef<'_>, ChunkGetError> {
        if !chunk_pos_in_bounds(chunk_pos) {
            return Err(ChunkGetError::out_of_bounds(chunk_pos));
        }

        let chunk = self
            .storage
            .get(chunk_pos)
            .ok_or(ChunkGetError::NotLoaded(chunk_pos))?;

        Ok(ChunkRef {
            chunk,
            stats: todo!(), // TODO: &self.statuses,
            entity: None,
        })
    }

    /// Create a new primordial chunk at the given position. Does not load or unload any chunks, rather
    /// this function uses the manager's settins to create a pre-configured chunk that can be loaded seperately.
    #[inline]
    pub(super) fn new_primordial_chunk(&self, chunk_pos: ChunkPos) -> Chunk {
        Chunk::new(chunk_pos, self.default_block, ChunkFlags::PRIMORDIAL)
    }
}
