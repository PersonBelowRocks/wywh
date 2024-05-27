use std::time::Instant;

use bevy::{
    ecs::system::SystemParam,
    math::{ivec2, ivec3},
    prelude::*,
    tasks::ComputeTaskPool,
};
use cb::channel;

use crate::{
    render::meshing::controller::MeshGeneration,
    topo::world::{realm::ChunkManagerResource, ChunkEntity, ChunkPos, VoxelRealm},
    util::{ws_to_chunk_pos, ChunkMap, ChunkSet},
};

use super::{
    ChunkObserver, ChunkObserverCrossChunkBorderEvent, ChunkObserverMoveEvent, ChunkPermitKey,
    Entry, LastPosition, LoadChunkEvent, LoadReasons, Permit, PermitFlags, UnloadChunkEvent,
    UpdatePermitEvent,
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
    let observer_pos = chunk_pos_center_vec3(observer_pos);
    let chunk_pos = chunk_pos_center_vec3(chunk_pos);

    let observer_horizontal_range_sq =
        observer_settings.horizontal_range * observer_settings.horizontal_range;

    let (vny, vpy) = (
        observer_settings.view_distance_below,
        observer_settings.view_distance_above,
    );

    let is_below = chunk_pos.y < observer_pos.y;

    (match is_below {
        true => (chunk_pos.y - observer_pos.y).abs() < vny,
        false => (chunk_pos.y - observer_pos.y).abs() < vpy,
    }) && (observer_pos.xz().distance_squared(chunk_pos.xz()) < observer_horizontal_range_sq)
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

    for (chunk_pos, entry) in removed.iter() {
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
        let observer_pos = chunk_pos_center_vec3(opos);

        let min_y = (observer_pos.y - observer.view_distance_below).floor() as i32;
        let max_y = (observer_pos.y + observer.view_distance_above).floor() as i32;

        let horizontal_range = observer.horizontal_range.floor() as i32;
        let horizontal_range_sq = observer.horizontal_range.powi(2).floor();
        let min_xz = opos.as_ivec3().xz() - IVec2::splat(horizontal_range);
        let max_xz = opos.as_ivec3().xz() + IVec2::splat(horizontal_range);

        for y in min_y..max_y {
            for x in min_xz.x..max_xz.x {
                for z in min_xz.y..max_xz.y {
                    if ivec2(x, z).as_vec2().distance_squared(observer_pos.xz())
                        > horizontal_range_sq
                    {
                        continue;
                    }

                    let cpos = ChunkPos::new(x, y, z);

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