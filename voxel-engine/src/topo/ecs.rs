use bevy::prelude::*;

use crate::{
    render::meshing::controller::{GrantPermit, RevokePermit},
    util::{ws_to_chunk_pos, ChunkSet},
};

use super::{
    world::{ChunkPos, VoxelRealm},
    worldgen::generator::GenerateChunk,
};

#[derive(Clone, Component, Debug)]
pub struct ChunkObserver {
    pub horizontal_range: f32,
    pub vertical_range: f32,
}

#[derive(Clone, Component, Debug)]
pub struct LastPosition {
    pub ws_pos: Vec3,
    pub chunk_pos: ChunkPos,
}

#[derive(Clone, Event, Debug)]
pub struct ChunkObserverMoveEvent {
    /// Indicates if this observer entity was just inserted.
    /// i.e. instead of a regular movement where its current position was different from its last position,
    /// this movement event was because the entity didn't even have a last position, and this was the first time
    /// we recorded its position.
    pub new: bool,
    pub entity: Entity,
    pub old_pos: Vec3,
    pub new_pos: Vec3,
}

#[derive(Clone, Event, Debug)]
pub struct ChunkObserverCrossChunkBorderEvent {
    /// Same as for ChunkObserverMoveEvent.
    pub new: bool,
    pub entity: Entity,
    pub old_chunk: ChunkPos,
    pub new_chunk: ChunkPos,
}

fn transform_chunk_pos(trans: &Transform) -> ChunkPos {
    ws_to_chunk_pos(trans.translation.floor().as_ivec3())
}

/// Dispatch movement events for chunk observers.
pub fn dispatch_move_events(
    mut observers: Query<(
        Entity,
        &Transform,
        &ChunkObserver,
        Option<&mut LastPosition>,
    )>,
    mut move_events: EventWriter<ChunkObserverMoveEvent>,
    mut chunk_border_events: EventWriter<ChunkObserverCrossChunkBorderEvent>,
    mut cmds: Commands,
) {
    for (entity, transform, observer, last_pos) in &mut observers {
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

pub fn remove_out_of_range_chunks(
    realm: Res<VoxelRealm>,
    mut border_events: EventReader<ChunkObserverCrossChunkBorderEvent>,
    mut revoke_permits: EventWriter<RevokePermit>,
) {
    todo!()
}

pub fn load_in_range_chunks(
    realm: Res<VoxelRealm>,
    mut border_events: EventReader<ChunkObserverCrossChunkBorderEvent>,
    mut revoke_permits: EventWriter<GrantPermit>,
) {
    todo!()
}
