use std::time::Instant;

use bevy::{prelude::*, render::primitives::Aabb};

use crate::{
    topo::{
        controller::ChunkPermitKey,
        world::{
            chunk_manager::{ChunkLoadResult, ChunkUnloadResult},
            Chunk, ChunkEntity, ChunkPos, VoxelRealm,
        },
    },
    util::ChunkMap,
};

use super::{
    AddPermitFlagsEvent, ChunkEcsPermits, LoadChunksEvent, LoadedChunkEvent, LoadshareMap,
    LoadshareProvider, MergeEvent, Permit, PermitLostFlagsEvent, RemovePermitFlagsEvent,
    UnloadChunksEvent, UnloadedChunkEvent, WorldControllerSettings,
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
    mut lost_flags_events: EventWriter<PermitLostFlagsEvent>,
    mut permits: ResMut<ChunkEcsPermits>,
    mut cmds: Commands,
) {
    for event in permit_events.read() {
        // Ignore (and warn about) events with empty flags, these are no-ops and probably a bug
        if event.remove_flags.is_empty() {
            warn!("Received permit flag removal event with empty permit flags. This is slightly sketchy and might indicate
            that something has gone wrong somewhere. No permits were updated and no ECS chunks were removed.");
            continue;
        };

        for chunk in event.chunks.iter() {
            let Some(permit) = permits.get_mut(ChunkPermitKey::Chunk(chunk)) else {
                // just ignore every chunk that doesn't have a permit, we're removing chunks here
                // so it would just get removed anyway
                continue;
            };

            let old_cached_flags = permit.cached_flags;

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

                // We can't gain any flags for a permit in this system, so the result of this XOR has to be
                // the flags that were removed.
                let lost_cached_flags = permit.cached_flags ^ old_cached_flags;
                if !lost_cached_flags.is_empty() {
                    lost_flags_events.send(PermitLostFlagsEvent {
                        chunk_pos: chunk,
                        lost_flags: lost_cached_flags,
                    });
                }

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
    for event in permit_events.read() {
        // Avoid adding flags to loadshares that don't exist anymore, all resources associated with these
        // has to be removed.
        if !loadshares.contains(event.loadshare) {
            continue;
        }

        // Ignore (and warn about) events with empty flags, these are no-ops and probably a bug
        if event.add_flags.is_empty() {
            warn!("Received permit flag addition event with empty permit flags. This is slightly sketchy and might indicate
            that something has gone wrong somewhere. No permits were updated and no ECS chunks were inserted.");
            continue;
        };

        for chunk in event.chunks.iter() {
            match permits.get_mut(ChunkPermitKey::Chunk(chunk)) {
                Some(permit) => {
                    permit
                        .loadshares
                        .entry(event.loadshare)
                        .and_modify(|ls_flags| ls_flags.insert(event.add_flags))
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

fn update_backlog<E: Event + Clone>(reader: &mut EventReader<E>, backlog: &mut Vec<E>) {
    backlog.extend(reader.read().cloned());
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
    mut unload_backlog: Local<Vec<UnloadChunksEvent>>,
    mut load_backlog: Local<Vec<LoadChunksEvent>>,
) {
    let threshold = settings.chunk_loading_handler_backlog_threshold;
    let timeout = settings.chunk_loading_handler_timeout;
    let max_stall = settings.chunk_loading_max_stalling;

    update_backlog(&mut unload_events, &mut unload_backlog);
    update_backlog(&mut load_events, &mut load_backlog);

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
    if unload_backlog.is_empty() || load_backlog.is_empty() {
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

            for event in load_backlog.drain(..) {
                // Skip this event if the event loadshare doesn't exist.
                if !realm.has_loadshare(event.loadshare) {
                    continue;
                }

                let mut loadshare = access.loadshare(event.loadshare);

                for chunk_pos in event.chunks.into_iter() {
                    let result = match loadshare.load_chunk(chunk_pos, event.reasons) {
                        Ok(result) => result,
                        Err(error) => {
                            error!("Error LOADING chunk at position {}: {error}", chunk_pos);
                            continue;
                        }
                    };

                    // If the chunk wasn't loaded before and the event wants to generate the chunk,
                    // dispatch a generation event.
                    if result == ChunkLoadResult::New {
                        loaded_chunks.send(LoadedChunkEvent {
                            chunk_pos: chunk_pos,
                            auto_generate: event.auto_generate,
                        });
                    }
                }
            }

            for event in unload_backlog.drain(..) {
                let mut loadshare = access.loadshare(event.loadshare);

                for chunk_pos in event.chunks.into_iter() {
                    match loadshare.unload_chunk(chunk_pos, event.reasons) {
                        Ok(result) => {
                            if matches!(result, ChunkUnloadResult::Unloaded) {
                                unloaded_chunks.send(UnloadedChunkEvent { chunk_pos });
                            }
                        }

                        Err(error) => {
                            // FIXME: sometimes we end up here with the error "chunk does not exist".
                            // figure out what causes this and what to do about it. it doesnt seem to be causing
                            // any issues but it's an error nonetheless
                            error!("Error UNLOADING chunk at position {}: {error}", chunk_pos);
                            continue;
                        }
                    }
                }
            }
        });
}
