use std::time::Instant;

use bevy::{
    math::{bounding::BoundingVolume, ivec3},
    prelude::*,
};

use crate::{
    topo::{
        world::{Chunk, ChunkPos, VoxelRealm},
        worldgen::{generator::GenerateChunk, GenerationPriority},
    },
    util::{ws_to_chunk_pos, ChunkMap, ChunkSet},
};

use super::{
    ChunkObserver, ChunkObserverCrossChunkBorderEvent, ChunkObserverMoveEvent, ChunkPermitKey,
    Entry, LastPosition, LoadChunkEvent, LoadReasons, LoadedChunkEvent, PermitFlags,
    UnloadChunkEvent, UpdatePermitEvent,
};

fn transform_chunk_pos(trans: &Transform) -> ChunkPos {
    ws_to_chunk_pos(trans.translation.floor().as_ivec3())
}

/// Dispatch movement events for chunk observers.
pub fn dispatch_move_events(
    mut observers: Query<(Entity, &Transform, Option<&mut LastPosition>), With<ChunkObserver>>,
    mut move_events: EventWriter<ChunkObserverMoveEvent>,
    mut chunk_border_events: EventWriter<ChunkObserverCrossChunkBorderEvent>,
    mut cmds: Commands,
) {
    for (entity, transform, last_pos) in &mut observers {
        match last_pos {
            Some(mut last_pos) => {
                // First we test for regular moves
                if last_pos.ws_pos == transform.translation {
                    continue;
                }

                move_events.send(ChunkObserverMoveEvent {
                    new: false,
                    entity,
                    old_pos: last_pos.ws_pos,
                    new_pos: transform.translation,
                });

                last_pos.ws_pos = transform.translation;

                // In order for the observer to have crossed a chunk border, it must have
                // moved to begin with, so this event is "dependant" on the regular move event

                let chunk_pos = transform_chunk_pos(transform);
                if chunk_pos == last_pos.chunk_pos {
                    continue;
                }

                chunk_border_events.send(ChunkObserverCrossChunkBorderEvent {
                    new: false,
                    entity,
                    old_chunk: last_pos.chunk_pos,
                    new_chunk: chunk_pos,
                });

                last_pos.chunk_pos = chunk_pos;
            }
            None => {
                // If this entity doesn't have a LastPosition component we add one and send events
                // with "new" set to true. This indicates to any readers that the events are for entities
                // that were just inserted and didn't have any position history.
                move_events.send(ChunkObserverMoveEvent {
                    new: true,
                    entity,
                    old_pos: transform.translation,
                    new_pos: transform.translation,
                });

                let chunk_pos = transform_chunk_pos(transform);

                chunk_border_events.send(ChunkObserverCrossChunkBorderEvent {
                    new: true,
                    entity,
                    old_chunk: chunk_pos,
                    new_chunk: chunk_pos,
                });

                cmds.entity(entity).insert(LastPosition {
                    ws_pos: transform.translation,
                    chunk_pos,
                });
            }
        }
    }
}

fn chunk_pos_center_vec3(pos: ChunkPos) -> Vec3 {
    pos.as_vec3() + Vec3::splat(0.5)
}

fn is_in_range(
    observer_pos: ChunkPos,
    chunk_pos: ChunkPos,
    observer_settings: &ChunkObserver,
) -> bool {
    let horizontal = Vec2::splat(observer_settings.horizontal_range);
    let above = observer_settings.view_distance_above;
    let below = -observer_settings.view_distance_below;

    let observer_pos = chunk_pos_center_vec3(observer_pos);
    let chunk_pos = chunk_pos_center_vec3(chunk_pos);

    let local_cpos = chunk_pos - observer_pos;

    let in_horizontal_range =
        local_cpos.xz().cmpge(-horizontal).all() && local_cpos.xz().cmple(horizontal).all();

    let in_vertical_range = (below..=above).contains(&local_cpos.y);

    in_horizontal_range && in_vertical_range
}

pub fn unload_out_of_range_chunks(
    realm: VoxelRealm,
    mut border_events: EventReader<ChunkObserverCrossChunkBorderEvent>,
    mut update_permits: EventWriter<UpdatePermitEvent>,
    mut unload_chunks: EventWriter<UnloadChunkEvent>,
    chunk_observers: Query<&ChunkObserver>,
) {
    let then = Instant::now();

    let mut moved_observers = ChunkMap::<&ChunkObserver>::default();
    for event in border_events.read() {
        if event.new {
            continue;
        }

        let Ok(observer) = chunk_observers.get(event.entity) else {
            error!("Chunk observer entity described in move event didn't exist in the query");
            continue;
        };

        moved_observers.set(event.new_chunk, observer);
    }

    // If there's no non-new border events, we don't do anything
    if moved_observers.len() <= 0 {
        return;
    }

    let mut removed = ChunkMap::<Entry>::new();

    for entry in realm.permits().iter() {
        let mut visible = false;
        // This would lead to a bug if we didn't already verify that there are actually events to be handled.
        // If 'moved_observers' is empty, then 'visible' remains false, and the chunk is unloaded.
        for (opos, &observer) in moved_observers.iter() {
            if is_in_range(opos, entry.chunk, observer) {
                visible = true;
                break;
            }
        }

        if !visible {
            removed.set(entry.chunk, entry.clone());
        }
    }

    for (chunk_pos, _entry) in removed.iter() {
        // TODO: fix unloading
        unload_chunks.send(UnloadChunkEvent {
            chunk_pos,
            reasons: LoadReasons::RENDER,
        });
        update_permits.send(UpdatePermitEvent {
            chunk_pos,
            insert_flags: PermitFlags::empty(),
            remove_flags: PermitFlags::RENDER,
        });
    }

    let now = Instant::now();
    let elapsed = now - then;

    if removed.len() > 0 {
        info!(
            "Spent {}ms unloading out of range chunks for observers",
            elapsed.as_millis()
        );
    }
}

pub fn load_in_range_chunks(
    realm: VoxelRealm,
    mut border_events: EventReader<ChunkObserverCrossChunkBorderEvent>,
    mut load_chunks: EventWriter<LoadChunkEvent>,
    mut update_permits: EventWriter<UpdatePermitEvent>,
    chunk_observers: Query<&ChunkObserver>,
) {
    let then = Instant::now();

    let mut moved_observers = ChunkMap::<&ChunkObserver>::default();
    for event in border_events.read() {
        let Ok(observer) = chunk_observers.get(event.entity) else {
            error!("Chunk observer entity described in move event didn't exist in the query");
            continue;
        };

        moved_observers.set(event.new_chunk, observer);
    }

    let mut in_range = ChunkSet::default();

    for (opos, &observer) in moved_observers.iter() {
        let min_y = (-observer.view_distance_below).floor() as i32;
        let max_y = observer.view_distance_above.ceil() as i32;

        let horizontal_min = IVec2::splat((-observer.horizontal_range).floor() as i32);
        let horizontal_max = IVec2::splat(observer.horizontal_range.ceil() as i32);

        for y in min_y..=max_y {
            for x in horizontal_min.x..=horizontal_max.x {
                for z in horizontal_min.y..=horizontal_max.y {
                    let pos = ivec3(x, y, z);
                    let cpos = ChunkPos::from(pos + opos.as_ivec3());

                    if !is_in_range(opos, cpos, observer) {
                        continue;
                    }

                    if realm
                        .permits()
                        .get(ChunkPermitKey::Chunk(cpos))
                        .is_some_and(|permit| permit.flags.contains(PermitFlags::RENDER))
                    {
                        continue;
                    }

                    in_range.set(cpos);
                }
            }
        }
    }

    for chunk_pos in in_range.iter() {
        load_chunks.send(LoadChunkEvent {
            chunk_pos,
            reasons: LoadReasons::RENDER,
            auto_generate: true,
        });
        update_permits.send(UpdatePermitEvent {
            chunk_pos,
            insert_flags: PermitFlags::RENDER,
            remove_flags: PermitFlags::empty(),
        });
    }

    let now = Instant::now();
    let elapsed = now - then;

    if in_range.len() > 0 {
        info!(
            "Spent {}ms loading in-range chunks for observers",
            elapsed.as_millis()
        );
    }
}

fn calculate_priority(trans: &Transform, chunk_pos: ChunkPos) -> GenerationPriority {
    const CHUNK_SIZE_F32: f32 = Chunk::SIZE as f32;
    const CHUNK_SIZE_DIV2: f32 = CHUNK_SIZE_F32 / 2.0;

    let chunk_center = (chunk_pos.as_vec3() * CHUNK_SIZE_F32) + Vec3::splat(CHUNK_SIZE_DIV2);

    let distance_sq = chunk_center.distance_squared(trans.translation);
    let distance_sq_int = distance_sq.clamp(0.0, u32::MAX as _) as u32;

    GenerationPriority::new(distance_sq_int)
}

pub fn generate_chunks_with_priority(
    observers: Query<&Transform, With<ChunkObserver>>,
    mut loaded_chunks: EventReader<LoadedChunkEvent>,
    mut generation_events: EventWriter<GenerateChunk>,
) {
    let mut chunks_to_gen = ChunkSet::default();

    // We only care about auto_generate chunks
    for chunk in loaded_chunks.read() {
        if chunk.auto_generate {
            chunks_to_gen.set(chunk.chunk_pos);
        }
    }

    generation_events.send_batch(chunks_to_gen.iter().map(|chunk_pos| {
        // Calculate priority based on distance to nearest observer, if there's no observers we use
        // the lowest priority.
        let priority = observers
            .iter()
            .map(|trans| calculate_priority(trans, chunk_pos))
            .max()
            .unwrap_or(GenerationPriority::LOWEST);

        GenerateChunk {
            pos: chunk_pos,
            priority,
        }
    }));
}
