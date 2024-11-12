use crate::data::registries::block::{BlockVariantId, BlockVariantRegistry};
use crate::topo::generic_chunk::GenericChunkReadAccess;
use crate::topo::world::chunk::ChunkData;
use crate::topo::world::ChunkDataError;
use bevy::math::IVec3;

/// A mock chunk that behaves like a regular chunk but can be trivially initialized and
/// has no relationship to a chunk manager. In a way this is just a glorified 3D array.
#[derive(Clone)]
pub struct MockChunk {
    data: ChunkData,
}

impl MockChunk {
    /// A "void" block. The default block in a chunk.
    pub const VOID: BlockVariantId = BlockVariantRegistry::VOID;
    pub const EXAMPLE1: BlockVariantId = BlockVariantId::new(1);
    pub const EXAMPLE2: BlockVariantId = BlockVariantId::new(2);
    pub const EXAMPLE3: BlockVariantId = BlockVariantId::new(3);

    /// Create a new mock chunk, filled with [`MockChunk::VOID`] blocks.
    pub fn new() -> Self {
        Self {
            data: ChunkData::new(Self::VOID),
        }
    }

    pub fn set(&mut self, ls_pos: IVec3, data: BlockVariantId) -> Result<(), ChunkDataError> {
        self.data.set(ls_pos, data)
    }

    pub fn set_mb(&mut self, mb_pos: IVec3, data: BlockVariantId) -> Result<(), ChunkDataError> {
        self.data.set_mb(mb_pos, data)
    }

    pub fn get(&self, ls_pos: IVec3) -> Result<Option<BlockVariantId>, ChunkDataError> {
        match self.data.get(ls_pos) {
            Ok(block) => Ok(Some(block)),
            Err(ChunkDataError::NonFullBlock) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn get_mb(&self, mb_pos: IVec3) -> Result<BlockVariantId, ChunkDataError> {
        self.data.get_mb(mb_pos)
    }
}

impl GenericChunkReadAccess for MockChunk {
    type Error = ChunkDataError;

    fn get(&self, ls_pos: IVec3) -> Result<Option<BlockVariantId>, Self::Error> {
        self.get(ls_pos)
    }

    fn get_mb(&self, mb_pos: IVec3) -> Result<BlockVariantId, Self::Error> {
        self.get_mb(mb_pos)
    }
}
