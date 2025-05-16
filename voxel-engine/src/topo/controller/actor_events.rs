use std::time::Duration;

use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    prelude::*,
    time::Stopwatch,
};
use itertools::Itertools;

use super::{
    ActorLoadRegion, CrossChunkBorder, LoadChunksEvent, LoadedChunkEvent, PreviousActorPosition,
    UnloadChunksEvent,
};
use crate::topo::fb_worldspace_to_chunkspace;
use crate::{
    topo::world::{
        ChunkPos,
        chunk_manager::ChunkLoadResult,
        chunk_populator::events::{
            PopulateChunk, PriorityCalcStrategy, RecalculatePopulateEventPrioritiesEvent,
        },
    },
    util::closest_distance_sq,
};

/// Convert a transform's translation to a chunk position
#[inline]
fn translation_to_chunk_pos(translation: Vec3) -> ChunkPos {
    ChunkPos::from(fb_worldspace_to_chunkspace(translation.floor().as_ivec3()))
}

/// Trigger [`CrossChunkBorder`] events for actors when they cross a chunk border.
/// Uses the [`PreviousActorPosition`] component on actor entities to determine when a border has been crossed.
pub fn trigger_actor_chunk_border_events(
    q_actors: Query<(Entity, &Transform), With<ActorLoadRegion>>,
    mut q_previous_actor_positions: Query<&mut PreviousActorPosition>,
    mut commands: Commands,
) {
    for (actor, transform) in &q_actors {
        let current_chunk_pos = translation_to_chunk_pos(transform.translation);

        let Ok(mut previous_position) = q_previous_actor_positions.get_mut(actor) else {
            // no previous position component on this actor, so insert one!
            // inserting this component for the first time will also trigger a CrossChunkBorder event
            commands.entity(actor).insert(PreviousActorPosition {
                ws_pos: transform.translation,
                chunk_pos: current_chunk_pos,
            });

            continue;
        };

        if current_chunk_pos == previous_position.chunk_pos {
            // no chunk border has been crossed, skip!
            continue;
        }

        // chunk border has been crossed, so trigger the event!
        commands.trigger_targets(
            CrossChunkBorder {
                new: false,
                old_chunk: previous_position.chunk_pos,
                new_chunk: current_chunk_pos,
            },
            actor,
        );

        // update the previous chunk position
        previous_position.chunk_pos = current_chunk_pos;
    }
}

/// System for dispatching population events for newly loaded chunks.
pub fn populate_loaded_chunks(
    q_observers: Query<&Transform, With<ActorLoadRegion>>,
    mut loaded_chunk_events: EventReader<LoadedChunkEvent>,
    mut populate_chunk_events: EventWriter<PopulateChunk>,
) {
    for loaded in loaded_chunk_events.read() {
        // Don't send population events for revived chunks or chunks that don't want to be automatically populated.
        // Revived chunks are handled by another system so that their meshes are built.
        if !loaded.auto_populate || matches!(loaded.load_result, ChunkLoadResult::Revived) {
            continue;
        }

        let center = loaded.chunk_pos.worldspace_center();
        let observer_positions = q_observers.iter().map(|&transform| transform.translation);
        let min_distance_sq = closest_distance_sq(center, observer_positions).unwrap_or(0.0);

        populate_chunk_events.write(PopulateChunk {
            chunk_pos: loaded.chunk_pos,
            // Closer chunk positions are higher priority, so we need to invert the distance.
            priority: u32::MAX - (min_distance_sq.ceil() as u32),
        });
    }
}

/// The distance an observer must have traveled for a priority recalculation to be forced.
pub const FORCE_RECALC_PRIORITY_DISTANCE: f32 = 125.0;
/// The distance an observer must have traveled for a priority recalculation to happen if [`RECALC_PRIORITY_INTERVAL`]
/// time has elapsed since the last recalculation.
pub const RECALC_PRIORITY_DISTANCE: f32 = 8.0;
/// The time that must have elapsed in order for a priority recalculation to happen.
pub const RECALC_PRIORITY_INTERVAL: Duration = Duration::from_millis(2000);

pub fn send_priority_recalculation_events(
    time: Res<Time<Real>>,
    q_observers: Query<(Entity, &Transform), With<ActorLoadRegion>>,
    mut population_events: EventWriter<RecalculatePopulateEventPrioritiesEvent>,
    mut previous_observer_positions: Local<EntityHashMap<Vec3>>,
    mut time_since_last_send: Local<Stopwatch>,
) {
    time_since_last_send.tick(time.delta());

    // This is used to track the "active" observers so that we remove observers from 'previous_observer_positions'
    // when they are no longer in the world.
    let mut active = EntityHashSet::with_capacity(previous_observer_positions.len());

    let mut observer_positions = Vec::<(Entity, Vec3)>::new();

    let mut should_send = false;
    for (observer_entity, &transform) in &q_observers {
        let current_pos = transform.translation;
        active.insert(observer_entity);

        if let Some(&previous_pos) = previous_observer_positions.get(&observer_entity) {
            let distance_sq = previous_pos.distance_squared(current_pos);

            should_send |= time_since_last_send.elapsed() >= RECALC_PRIORITY_INTERVAL
                && distance_sq >= RECALC_PRIORITY_DISTANCE.powi(2);

            should_send |= distance_sq >= FORCE_RECALC_PRIORITY_DISTANCE.powi(2);
        } else {
            // Record this observer position as the previous one if there was none from before.
            previous_observer_positions.insert(observer_entity, current_pos);
        }

        observer_positions.push((observer_entity, current_pos));
    }

    if should_send {
        // Clear previous positions and reset the elapsed time.
        // We'll insert the current positions as the previous ones once we've sent the events.
        previous_observer_positions.clear();
        time_since_last_send.reset();

        population_events.write(RecalculatePopulateEventPrioritiesEvent {
            strategy: PriorityCalcStrategy::ClosestDistanceSq(
                observer_positions.iter().map(|(_, p)| *p).collect_vec(),
            ),
        });

        previous_observer_positions.extend(observer_positions.into_iter())
    }

    previous_observer_positions.retain(|observer, _| active.contains(observer));
}
