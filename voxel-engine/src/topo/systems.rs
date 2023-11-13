use bevy::prelude::*;

use crate::{data::tile::VoxelId, DefaultGenerator};

use super::{
    chunk::Chunk, generator::GenerateChunk, realm::VoxelRealm,
    storage::containers::dense::DenseChunkContainer,
};

pub(crate) fn generate_chunks_from_events(
    mut reader: EventReader<GenerateChunk<VoxelId>>,
    realm: Res<VoxelRealm>,
    generator: Res<DefaultGenerator>,
) {
    for event in reader.read() {
        let mut container = DenseChunkContainer::<VoxelId>::Empty;
        let mut access = container.auto_access(event.default_value);

        generator.write_to_chunk(event.pos, &mut access).unwrap();

        let chunk = Chunk::new_from_container(container);
        realm.chunk_manager.set_loaded_chunk(event.pos, chunk)
    }
}
