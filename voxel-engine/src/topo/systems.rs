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
        let cpos = event.pos;

        match realm.chunk_manager.initialize_new_chunk(event.pos) {
            Ok(cref) => {
                let result = cref.with_access(|access| generator.write_to_chunk(event.pos, access));

                match result {
                    Ok(Ok(_)) => (),
                    Err(error) => error!("Error getting write access to chunk '{cpos}': {error}"),
                    Ok(Err(error)) => {
                        error!("Generator raised an error generating chunk '{cpos}': {error}")
                    }
                }
            }
            Err(error) => error!("Error trying to generate chunk at '{cpos}': {error}"),
        }
    }
}
