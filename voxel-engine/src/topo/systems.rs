use bevy::prelude::*;

use crate::{data::tile::VoxelId, DefaultGenerator};

use super::{
    chunk::Chunk,
    generator::{GenerateChunk, GeneratorInput},
    realm::VoxelRealm,
    storage::containers::dense::DenseChunkContainer,
};

pub(crate) fn generate_chunks_from_events(
    mut reader: EventReader<GenerateChunk<VoxelId>>,
    realm: Res<VoxelRealm>,
    generator: Res<DefaultGenerator>,
) {
    for event in reader.read() {
        let mut input = GeneratorInput::new();
        let mut access = input.access(VoxelId::VOID);

        generator.write_to_chunk(event.pos, &mut access).unwrap();

        let chunk = input.to_chunk();
        realm.chunk_manager.set_loaded_chunk(event.pos, chunk)
    }
}
