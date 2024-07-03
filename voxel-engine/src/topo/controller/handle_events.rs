use std::time::Instant;

use bevy::{prelude::*, render::primitives::Aabb};

use crate::topo::world::{
    chunk_manager::{ChunkLoadResult, ChunkUnloadResult},
    Chunk, ChunkEntity, ChunkPos, VoxelRealm,
};

use super::{
    LoadChunks, LoadedChunkEvent, LoadshareMap, LoadshareProvider, UnloadChunks,
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
    mut load_events: EventReader<LoadChunks>,
    mut loaded_chunks: EventWriter<LoadedChunkEvent>,
    mut unload_events: EventReader<UnloadChunks>,
    mut unloaded_chunks: EventWriter<UnloadedChunkEvent>,
    // Backlogs
    mut unload_backlog: Local<Vec<UnloadChunks>>,
    mut load_backlog: Local<Vec<LoadChunks>>,
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
