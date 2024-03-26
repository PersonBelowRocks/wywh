use crate::topo::{
    block::BlockVoxel,
    chunk_ref::{ChunkRefVxlAccess, CrVra},
    storage::containers::data_storage::SyncIndexedChunkContainer,
};

pub struct MockChunk {
    pub variants: SyncIndexedChunkContainer<BlockVoxel>,
}

impl MockChunk {
    pub fn new(filling: BlockVoxel) -> Self {
        Self {
            variants: SyncIndexedChunkContainer::filled(filling),
        }
    }

    pub fn access(&self) -> ChunkRefVxlAccess<'_> {
        ChunkRefVxlAccess {
            block_variants: self.variants.access(),
        }
    }

    pub fn read_access(&self) -> CrVra<'_> {
        CrVra {
            block_variants: self.variants.read_access(),
        }
    }
}
