use std::time::Instant;

use bevy::{
    prelude::*,
    render::{primitives::Aabb, view::NoFrustumCulling},
};

use crate::{
    topo::{
        controller::{ChunkPermitKey, LoadReasons},
        world::{Chunk, ChunkEntity, ChunkManagerError, ChunkPos, VoxelRealm},
        worldgen::generator::GenerateChunk,
    },
    util::{ChunkMap, ChunkSet},
};

use super::{ChunkEcsPermits, LoadChunkEvent, Permit, UnloadChunkEvent, UpdatePermitEvent};

#[derive(Bundle)]
pub struct ChunkEcsBundle {
    pub chunk_pos: ChunkPos,
    pub marker: ChunkEntity,
    pub no_frustum_culling: NoFrustumCulling,
    pub aabb: Aabb,
    pub spatial: SpatialBundle,
}

impl ChunkEcsBundle {
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            chunk_pos: pos,
            marker: ChunkEntity,
            no_frustum_culling: NoFrustumCulling,
            aabb: Chunk::BOUNDING_BOX.to_aabb(),
            spatial: SpatialBundle {
                visibility: Visibility::Visible,
                transform: Transform::from_translation(pos.worldspace_min().as_vec3()),
                ..default()
            },
        }
    }
}

pub fn handle_permit_updates(
    mut permit_events: EventReader<UpdatePermitEvent>,
    mut permits: ResMut<ChunkEcsPermits>,
    chunks: Query<(Entity, &ChunkPos), With<ChunkEntity>>,
    mut cmds: Commands,
) {
    let then = Instant::now();

    let has_events = permit_events.len() > 0;

    let mut permit_updates = ChunkMap::<UpdatePermitEvent>::with_capacity(permit_events.len());

    for event in permit_events.read().copied() {
        match permit_updates.get_mut(event.chunk_pos) {
            // If an event for this position had already been encountered, merge the flags together with this one.
            Some(existing_event) => {
                existing_event.insert_flags.insert(event.insert_flags);
                existing_event.remove_flags.insert(event.remove_flags);
            }
            // We haven't seen this position before, add it to the map
            None => {
                permit_updates.set(event.chunk_pos, event);
            }
        }
    }

    // update or set permits for ECS chunks
    for (entity, &chunk) in &chunks {
        let Some(permit) = permits.get_mut(ChunkPermitKey::Chunk(chunk)) else {
            error!(
                "Detected chunk entity in ECS world without a permit, so the entity was despawned."
            );
            cmds.entity(entity).despawn();
            continue;
        };

        // remove permit updates as we go, this lets us isolate the permit updates that
        // don't have an existing ECS chunk
        let Some(event) = permit_updates.remove(chunk) else {
            continue;
        };

        // lil sanity check just to be sure
        assert_eq!(chunk, event.chunk_pos);

        // Update the permit's flags
        permit.flags.insert(event.insert_flags);
        permit.flags.remove(event.remove_flags);

        // Remove the permit (and remove the ECS chunk) if the flags ended up being empty.
        if permit.flags.is_empty() {
            permits
                .remove(ChunkPermitKey::Chunk(event.chunk_pos))
                .map(|entry| cmds.entity(entry.entity).despawn());
        }
    }

    // we need to insert new ECS chunks for the permit events that are left over!
    for (chunk_pos, &permit) in permit_updates.iter() {
        let mut permit_flags = permit.insert_flags;
        permit_flags.remove(permit.remove_flags);

        if permit_flags.is_empty() {
            warn!("Received permit update event with empty (or cancelled out) permit flags. This is slightly sketchy and might indicate 
            that something has gone wrong somewhere. No permits were updated and no ECS chunks were inserted.");
            continue;
        };

        let entity = cmds.spawn(ChunkEcsBundle::new(chunk_pos)).id();

        permits.insert(
            entity,
            chunk_pos,
            Permit {
                flags: permit_flags,
            },
        );
    }

    let now = Instant::now();
    let elapsed = now - then;

    if has_events {
        info!("Spent {}ms handling permit updates", elapsed.as_millis());
    }
}

pub fn handle_chunk_loads(
    realm: VoxelRealm,
    mut load_events: EventReader<LoadChunkEvent>,
    mut generation_events: EventWriter<GenerateChunk>,
) {
    let then = Instant::now();
    let has_events = load_events.len() > 0;

    for &event in load_events.read() {
        let chunk_pos = event.chunk_pos;
        // TODO: dont load if theres no reasons
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

    let now = Instant::now();
    let elapsed = now - then;

    if has_events {
        info!("Spent {}ms handling chunk loads", elapsed.as_millis());
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

    let then = Instant::now();

    for removed_chunk in removed.iter() {
        if let Err(error) = realm.cm().unload_chunk(removed_chunk) {
            error!("Error unloading chunk at {removed_chunk}: {error}");
        }
    }

    let now = Instant::now();
    let elapsed = now - then;

    if removed.len() > 0 {
        info!("Spent {}ms handling chunk unloads", elapsed.as_millis());
    }
}
