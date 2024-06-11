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
    AddPermitFlagsEvent, ChunkEcsPermits, LoadChunksEvent, LoadedChunkEvent, LoadshareMap,
    LoadshareProvider, MergeEvent, Permit, RemovePermitFlagsEvent, UnloadChunksEvent,
    UnloadedChunkEvent, WorldControllerSettings,
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

/// System for handling permit flag removal events.
pub fn handle_permit_flag_removals(
    mut permit_events: EventReader<RemovePermitFlagsEvent>,
    mut permits: ResMut<ChunkEcsPermits>,
    mut cmds: Commands,
) {
    let mut removals = LoadshareMap::<RemovePermitFlagsEvent>::default();

    for event in permit_events.read() {
        if event.remove_flags.is_empty() {
            warn!("Received permit flag removal event with empty permit flags. This is slightly sketchy and might indicate
            that something has gone wrong somewhere. No permits were updated and no ECS chunks were removed.");
            continue;
        };

        removals
            .entry(event.loadshare)
            .and_modify(|existing| {
                // If an event for this loadshare had already been encountered, merge the flags and chunk positions together with this one.
                existing.remove_flags |= event.remove_flags;
                existing.chunks.extend(event.chunks.iter());
            })
            .or_insert(event.clone());
    }

    for event in removals.into_values() {
        for chunk in event.chunks.into_iter() {
            let Some(permit) = permits.get_mut(ChunkPermitKey::Chunk(chunk)) else {
                // just ignore every chunk that doesn't have a permit, we're removing chunks here
                // so it would just get removed anyway
                continue;
            };

            // update the permit flags for our loadshare (if it exists) and note down if this permit
            // was granted under that loadshare
            let loaded_under_loadshare = permit
                .loadshares
                .get_mut(&event.loadshare)
                .map(|flags| flags.remove(event.remove_flags))
                .is_some();

            // if the permit was granted under this loadshare and we removed some of its flags above
            // then we update its cached flags and check if we should remove the permit (and its ECS
            // entity) entirely.
            if loaded_under_loadshare {
                permit.update_cached_flags();

                if permit.cached_flags.is_empty() {
                    permits
                        .remove(ChunkPermitKey::Chunk(chunk))
                        .map(|entry| cmds.entity(entry.entity).despawn());
                }
            }
        }
    }
}

/// System for handling permit flag addition events.
pub fn handle_permit_flag_additions(
    mut permit_events: EventReader<AddPermitFlagsEvent>,
    mut permits: ResMut<ChunkEcsPermits>,
    loadshares: Res<LoadshareProvider>,
    mut cmds: Commands,
) {
    let mut additions = LoadshareMap::<AddPermitFlagsEvent>::default();

    for event in permit_events.read() {
        // Avoid adding flags to loadshares that don't exist anymore, all resources associated with these
        // has to be removed.
        if !loadshares.contains(event.loadshare) {
            continue;
        }

        if event.add_flags.is_empty() {
            warn!("Received permit flag addition event with empty permit flags. This is slightly sketchy and might indicate
            that something has gone wrong somewhere. No permits were updated and no ECS chunks were inserted.");
            continue;
        };

        additions
            .entry(event.loadshare)
            .and_modify(|existing| {
                // If an event for this loadshare had already been encountered, merge the flags and chunk positions together with this one.
                existing.add_flags |= event.add_flags;
                existing.chunks.extend(event.chunks.iter());
            })
            .or_insert(event.clone());
    }

    // walk through every event which had both a valid loadshare and non-empty flags
    for event in additions.into_values() {
        for chunk in event.chunks.into_iter() {
            match permits.get_mut(ChunkPermitKey::Chunk(chunk)) {
                Some(permit) => {
                    permit
                        .loadshares
                        .entry(event.loadshare)
                        .and_modify(|mut ls_flags| ls_flags.insert(event.add_flags))
                        .or_insert(event.add_flags);

                    permit.update_cached_flags();
                }
                None => {
                    let entity = cmds.spawn(ChunkEcsBundle::new(chunk)).id();

                    permits.insert(
                        entity,
                        chunk,
                        Permit {
                            cached_flags: event.add_flags,
                            loadshares: LoadshareMap::from_iter([(
                                event.loadshare,
                                event.add_flags,
                            )]),
                        },
                    );
                }
            }
        }
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
    mut load_events: EventReader<LoadChunksEvent>,
    mut loaded_chunks: EventWriter<LoadedChunkEvent>,
    mut unload_events: EventReader<UnloadChunksEvent>,
    mut unloaded_chunks: EventWriter<UnloadedChunkEvent>,
    // Backlogs
    mut unload_backlog: Local<ChunkMap<UnloadChunksEvent>>,
    mut load_backlog: Local<ChunkMap<LoadChunksEvent>>,
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
