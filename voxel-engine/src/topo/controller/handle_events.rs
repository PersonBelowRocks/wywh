use bevy::prelude::*;

use crate::{
    topo::{
        controller::{ChunkPermitKey, LoadReasons},
        world::{ChunkEntity, ChunkManagerError, ChunkPos, VoxelRealm},
        worldgen::generator::GenerateChunk,
    },
    util::{ChunkMap, ChunkSet},
};

use super::{ChunkEcsPermits, LoadChunkEvent, Permit, UnloadChunkEvent, UpdatePermit};

pub fn handle_permit_updates(
    mut permit_events: EventReader<UpdatePermit>,
    mut permits: ResMut<ChunkEcsPermits>,
    chunks: Query<(Entity, &ChunkPos), With<ChunkEntity>>,
    mut cmds: Commands,
) {
    let mut permit_updates = ChunkMap::<Permit>::with_capacity(permit_events.len());

    for event in permit_events.read() {
        permit_updates.set(event.chunk_pos, event.new_permit);
    }

    // update or set permits for ECS chunks
    for (entity, &chunk) in &chunks {
        // remove permit updates as we go, this lets us isolate the permit updates that aren't
        // don't have an existing ECS chunk
        let Some(permit) = permit_updates.remove(chunk) else {
            continue;
        };

        if permit.flags.is_empty() {
            permits
                .remove(ChunkPermitKey::Chunk(chunk))
                .map(|entry| cmds.entity(entry.entity).despawn());
        } else {
            match permits.get_mut(ChunkPermitKey::Chunk(chunk)) {
                Some(existing_permit) => *existing_permit = permit,
                // all ECS chunks present in the world should also have associated permits, but if they don't who cares who
                // are we to judge, we'll just help them out and insert one for them
                // TODO: might wanna log a warning in this case
                None => permits.insert(entity, chunk, permit),
            }
        }
    }

    // we need to insert new ECS chunks for the permit events that are left over!
    for (chunk_pos, &permit) in permit_updates.iter() {
        let entity = cmds
            .spawn((
                chunk_pos,
                ChunkEntity,
                VisibilityBundle {
                    visibility: Visibility::Visible,
                    ..default()
                },
            ))
            .id();

        permits.insert(entity, chunk_pos, permit);
    }
}

pub fn handle_chunk_loads(
    realm: VoxelRealm,
    mut load_events: EventReader<LoadChunkEvent>,
    mut generation_events: EventWriter<GenerateChunk>,
) {
    for &event in load_events.read() {
        let chunk_pos = event.chunk_pos;
        let result = realm.cm().initialize_new_chunk(chunk_pos, event.reasons);
        match result {
            Ok(_) => {
                // dispatch a generation event if needed
                if event.auto_generate {
                    generation_events.send(GenerateChunk { pos: chunk_pos });
                }
            }
            Err(ChunkManagerError::AlreadyInitialized) => {
                if let Err(error) = realm
                    .cm()
                    .get_loaded_chunk(chunk_pos, true)
                    .map(|cref| cref.update_load_reasons(|reasons| reasons.insert(event.reasons)))
                {
                    error!("Error when updating load reasons for chunk {chunk_pos}: {error}");
                    continue;
                }
            }
            Err(error) => {
                error!("Error initializing chunk at {chunk_pos} during chunk loading: {error}");
                continue;
            }
        }
    }
}

pub fn handle_chunk_unloads(realm: VoxelRealm, mut unload_events: EventReader<UnloadChunkEvent>) {
    // we can't unload chunks as we go because we're holding a lock guard to the chunk (and we'll deadlock)
    // so we keep track of everything that needs to be removed and do it all at the end
    let mut removed = ChunkSet::default();

    for &event in unload_events.read() {
        match realm.cm().get_loaded_chunk(event.chunk_pos, true) {
            Ok(cref) => {
                let new_reasons = cref.update_load_reasons(|flags| flags.remove(event.reasons));
                if new_reasons.is_empty() {
                    removed.set(event.chunk_pos);
                }
            }
            Err(error) => {
                error!(
                    "Error getting chunk at {} to unload: {error}",
                    event.chunk_pos
                );
                continue;
            }
        }
    }

    for removed_chunk in removed.iter() {
        if let Err(error) = realm.cm().unload_chunk(removed_chunk) {
            error!("Error unloading chunk at {removed_chunk}: {error}");
        }
    }
}
