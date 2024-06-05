use std::time::Instant;

use bevy::{prelude::*, render::primitives::Aabb};

use crate::{
    topo::{
        controller::ChunkPermitKey,
        world::{chunk_manager::ChunkLoadResult, Chunk, ChunkEntity, ChunkPos, VoxelRealm},
    },
    util::ChunkMap,
};

use super::{
    ChunkEcsPermits, LoadChunkEvent, LoadedChunkEvent, MergeEvent, Permit, UnloadChunkEvent,
    UnloadedChunkEvent, UpdatePermitEvent, WorldControllerSettings,
};

#[derive(Bundle)]
pub struct ChunkEcsBundle {
    pub chunk_pos: ChunkPos,
    pub marker: ChunkEntity,
    pub aabb: Aabb,
    pub spatial: SpatialBundle,
}

impl ChunkEcsBundle {
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            chunk_pos: pos,
            marker: ChunkEntity,
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

fn update_backlogs<E: Event + Clone + MergeEvent>(
    reader: &mut EventReader<E>,
    backlog: &mut ChunkMap<E>,
) {
    for event in reader.read() {
        backlog
            .entry(event.pos())
            .and_modify(|e| e.merge(event.clone()).unwrap())
            .or_insert(event.clone());
    }
}

pub fn handle_chunk_loads_and_unloads(
    // Prelude
    realm: VoxelRealm,
    settings: Res<WorldControllerSettings>,
    // Timekeeping
    time: Res<Time<Real>>,
    mut latest_cycle: Local<Option<Instant>>,
    // Events
    mut load_events: EventReader<LoadChunkEvent>,
    mut loaded_chunks: EventWriter<LoadedChunkEvent>,
    mut unload_events: EventReader<UnloadChunkEvent>,
    mut unloaded_chunks: EventWriter<UnloadedChunkEvent>,
    // Backlogs
    mut unload_backlog: Local<ChunkMap<UnloadChunkEvent>>,
    mut load_backlog: Local<ChunkMap<LoadChunkEvent>>,
) {
    let threshold = settings.chunk_loading_handler_backlog_threshold;
    let timeout = settings.chunk_loading_handler_timeout;
    let max_stall = settings.chunk_loading_max_stalling;

    update_backlogs(&mut unload_events, &mut unload_backlog);
    update_backlogs(&mut load_events, &mut load_backlog);

    let Some(now) = time.last_update() else {
        return;
    };

    // If we've stalled for more than our allowed time, we have to load the chunks ASAP, so we force
    // the global lock.
    let overtime = match *latest_cycle {
        Some(latest) if now - latest > max_stall => true,
        _ => false,
    };

    // Nothing to process, so just return early.
    if unload_backlog.len() <= 0 || load_backlog.len() <= 0 {
        return;
    }

    // Force a global lock if either of the backlogs exceeded their threshold or if we've stalled
    // for too long (see above)
    let force = load_backlog.len() > threshold || unload_backlog.len() > threshold || overtime;

    realm
        .cm()
        .with_global_lock(Some(timeout), force, |mut access| {
            // Record this cycle
            *latest_cycle = Some(now);

            for (chunk_pos, event) in unload_backlog.iter() {
                // sanity check to catch potential shenanigans early
                assert_eq!(chunk_pos, event.chunk_pos);

                match access.unload_chunk(event.chunk_pos, event.reasons) {
                    Ok(unloaded) => {
                        if unloaded {
                            // Remove this chunk from our backlog so we don't re-load it later on
                            // FIXME: unsure of this logic, we might have a condition where a chunk is
                            // unloaded and then loaded again before this system has a chance to do anything
                            // about it. in such a scenario the chunk should be loaded but we've removed it from
                            // the load backlog.
                            load_backlog.remove(event.chunk_pos);

                            unloaded_chunks.send(UnloadedChunkEvent {
                                chunk_pos: event.chunk_pos,
                            });
                        }
                    }
                    Err(error) => {
                        // FIXME: sometimes we end up here with the error "chunk does not exist".
                        // figure out what causes this and what to do about it. it doesnt seem to be causing
                        // any issues but it's an error nonetheless
                        error!(
                            "Error UNLOADING chunk at position {}: {error}",
                            event.chunk_pos
                        );
                        continue;
                    }
                }
            }

            // Clear the backlog, we just processed everything in it.
            unload_backlog.clear();

            for (chunk_pos, event) in load_backlog.iter() {
                // sanity check to catch potential shenanigans early
                assert_eq!(chunk_pos, event.chunk_pos);

                let result = match access.load_chunk(event.chunk_pos, event.reasons) {
                    Ok(result) => result,
                    Err(error) => {
                        error!(
                            "Error LOADING chunk at position {}: {error}",
                            event.chunk_pos
                        );
                        continue;
                    }
                };

                // If the chunk wasn't loaded before and the event wants to generate the chunk,
                // dispatch a generation event.
                if result == ChunkLoadResult::New {
                    loaded_chunks.send(LoadedChunkEvent {
                        chunk_pos: event.chunk_pos,
                        auto_generate: event.auto_generate,
                    });
                }
            }

            // Clear this backlog too.
            load_backlog.clear();
        });
}
