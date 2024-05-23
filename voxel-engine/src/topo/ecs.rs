use bevy::{
    ecs::{query::BatchingStrategy, system::SystemParam},
    math::{ivec2, ivec3},
    prelude::*,
    tasks::ComputeTaskPool,
};
use crossbeam::channel;

use crate::{
    render::meshing::controller::{ChunkRenderPermits, GrantPermit, MeshGeneration, RevokePermit},
    topo::world::chunk_entity::{CEBimapValue, Pair},
    util::{ws_to_chunk_pos, ChunkMap, ChunkSet},
};

use super::{
    world::{
        realm::{ChunkEntitiesBijectionResource, ChunkManagerResource},
        ChunkEntity, ChunkPos, VoxelRealm,
    },
    worldgen::generator::GenerateChunk,
};

#[derive(Clone, Component, Debug)]
pub struct ChunkObserver {
    pub horizontal_range: f32,
    pub view_distance_above: f32,
    pub view_distance_below: f32,
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

#[derive(SystemParam)]
pub struct MutVoxelRealm<'w> {
    chunk_manager: Res<'w, ChunkManagerResource>,
    chunk_entities: ResMut<'w, ChunkEntitiesBijectionResource>,
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

pub fn remove_out_of_range_chunks(
    mut realm: MutVoxelRealm,
    mut border_events: EventReader<ChunkObserverCrossChunkBorderEvent>,
    mut revoke_permits: EventWriter<RevokePermit>,
    chunk_entites: Query<(Entity, &ChunkPos), With<ChunkEntity>>,
    chunk_observers: Query<&ChunkObserver>,
    generation: Res<MeshGeneration>,
    mut cmds: Commands,
) {
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

    let (removed_tx, removed_rx) = channel::unbounded::<Pair>();
    chunk_entites.par_iter().for_each(|(entity, &cpos)| {
        let mut is_visible = false;

        for (opos, observer) in moved_observers.iter() {
            is_visible = is_in_range(opos, cpos, observer);

            if is_visible {
                break;
            }
        }

        if !is_visible {
            removed_tx
                .send(Pair {
                    entity,
                    chunk: cpos,
                })
                .unwrap();
        }
    });

    let mut total_removed = 0;
    while let Ok(removed) = removed_rx.recv() {
        realm
            .chunk_entities
            .0
            .remove(CEBimapValue::Entity(removed.entity));
        cmds.entity(removed.entity)
            .remove::<(ChunkEntity, ChunkPos)>();

        if let Err(error) = realm.chunk_manager.0.unload_chunk(removed.chunk) {
            error!(
                "Error when unloading chunk {} from chunk manager: {error}",
                removed.chunk
            );
        }

        revoke_permits.send(RevokePermit {
            pos: removed.chunk,
            generation: generation.0,
        });

        total_removed += 1;
    }

    if total_removed > 0 {
        debug!("Removed and unloaded {total_removed} chunks");
    }
}

pub fn load_in_range_chunks(
    realm: MutVoxelRealm,
    permits: Res<ChunkRenderPermits>,
    mut border_events: EventReader<ChunkObserverCrossChunkBorderEvent>,
    mut revoke_permits: EventWriter<GrantPermit>,
    chunk_observers: Query<&ChunkObserver>,
) {
    let mut moved_observers = ChunkMap::<&ChunkObserver>::default();
    for event in border_events.read() {
        let Ok(observer) = chunk_observers.get(event.entity) else {
            error!("Chunk observer entity described in move event didn't exist in the query");
            continue;
        };

        moved_observers.set(event.new_chunk, observer);
    }

    let pool = ComputeTaskPool::get();

    let mut results = Vec::new();

    for (opos, &observer) in moved_observers.iter() {
        let observer_pos = chunk_pos_center_vec3(opos);

        let min_y = (observer_pos.y - observer.view_distance_below).floor() as i32;
        let max_y = (observer_pos.y + observer.view_distance_above).floor() as i32;

        let horizontal_range = observer.horizontal_range.floor() as i32;
        let horizontal_range_sq = observer.horizontal_range.powi(2).floor();
        let min_xz = opos.as_ivec3().xz() - IVec2::splat(horizontal_range);
        let max_xz = opos.as_ivec3().xz() + IVec2::splat(horizontal_range);

        for y in min_y..max_y {
            let chunks = pool
                .scope(|scope| {
                    scope.spawn(async {
                        let mut positions = Vec::<ChunkPos>::new();

                        for x in min_xz.x..max_xz.x {
                            for z in min_xz.y..max_xz.y {
                                if ivec2(x, z).as_vec2().distance_squared(observer_pos.xz())
                                    > horizontal_range_sq
                                {
                                    continue;
                                }

                                let cpos = ChunkPos::new(ivec3(x, y, z));

                                if permits.has_permit(cpos) {
                                    continue;
                                }

                                positions.push(cpos);
                            }
                        }

                        positions
                    });
                })
                .concat();

            results.push(chunks);
        }
    }

    let mut load = ChunkSet::default();

    for chunks in results {
        for chunk in chunks {
            load.set(chunk);
        }
    }

    for chunk in load.iter() {
        todo!()
    }

    todo!()
}
