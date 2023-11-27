use bevy::prelude::*;

use crate::DefaultGenerator;

use super::{
    generator::{GenerateChunk, GeneratorInput},
    realm::VoxelRealm,
};

pub(crate) fn generate_chunks_from_events(
    mut reader: EventReader<GenerateChunk>,
    realm: Res<VoxelRealm>,
    generator: Res<DefaultGenerator>,
) {
    for event in reader.read() {
        let mut input = GeneratorInput::new();
        let mut access = input.access();

        generator.write_to_chunk(event.pos, &mut access).unwrap();

        let chunk = input.to_chunk();
        realm.chunk_manager.set_loaded_chunk(event.pos, chunk)
    }
}
