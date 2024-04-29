use crate::topo::{
    block::BlockVoxel,
    storage::containers::data_storage::SyncIndexedChunkContainer,
    world::{Crra, Crwa},
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

    pub fn access(&self) -> Crwa<'_> {
        Crwa {
            wrote_to_edge: None,
            block_variants: self.variants.access(),
        }
    }

    pub fn read_access(&self) -> Crra<'_> {
        Crra {
            block_variants: self.variants.read_access(),
        }
    }
}
