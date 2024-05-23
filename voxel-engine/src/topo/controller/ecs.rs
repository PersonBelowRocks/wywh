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
    ChunkObserver, ChunkObserverCrossChunkBorderEvent, ChunkObserverMoveEvent, LastPosition,
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
