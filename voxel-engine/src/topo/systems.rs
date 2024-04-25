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
    // TODO: parallelize!
    for event in reader.read() {
        let cpos = event.pos;

        match realm.chunk_manager.initialize_new_chunk(event.pos) {
            Ok(cref) => {
                let access_result = cref.with_access(|mut access| {
                    let gen_result = generator.write_to_chunk(event.pos, &mut access);

                    if let Err(error) = gen_result {
                        error!("Generator raised an error generating chunk '{cpos}': {error}")
                    } else {
                        access.coalesce_microblocks();
                        access.optimize_internal_storage();
                    }
                });

                if let Err(error) = access_result {
                    error!("Error getting write access to chunk '{cpos}': {error}");
                }
            }
            Err(error) => error!("Error trying to generate chunk at '{cpos}': {error}"),
        }
    }
}
