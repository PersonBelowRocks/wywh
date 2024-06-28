use std::sync::atomic::Ordering;

use bevy::{ecs::entity::EntityHashMap, math::ivec3, prelude::*};

use crate::{
    render::{ChunkBatch, ObserverBatches, VisibleBatches},
    topo::{
        bounding_box::BoundingBox,
        world::{Chunk, ChunkPos, VoxelRealm},
        worldgen::{generator::GenerateChunk, GenerationPriority},
    },
    util::{ws_to_chunk_pos, ChunkSet},
};

use super::{
    AddPermitFlagsEvent, ChunkPermitKey, CrossChunkBorder, LastPosition, LoadChunksEvent,
    LoadReasons, LoadedChunkEvent, LoadshareProvider, ObserverLoadshare, ObserverLoadshareType,
    ObserverSettings, PermitFlags, RemovePermitFlagsEvent, UnloadChunksEvent,
};

fn transform_chunk_pos(trans: &Transform) -> ChunkPos {
    ws_to_chunk_pos(trans.translation.floor().as_ivec3())
}

pub fn dispatch_move_events(
    mut observers: Query<(Entity, &Transform, Option<&mut LastPosition>), With<ObserverSettings>>,
    mut chunk_border_events: EventWriter<CrossChunkBorder>,
    mut cmds: Commands,
) {
    for (entity, transform, last_pos) in &mut observers {
        match last_pos {
            Some(mut last_pos) => {
                // First we test for regular moves
                if last_pos.ws_pos == transform.translation {
                    continue;
                }

                last_pos.ws_pos = transform.translation;

                // In order for the observer to have crossed a chunk border, it must have
                // moved to begin with, so this event is "dependant" on the regular move event

                let chunk_pos = transform_chunk_pos(transform);
                if chunk_pos == last_pos.chunk_pos {
                    continue;
                }

                cmds.trigger_targets(
                    CrossChunkBorder {
                        new: false,
                        old_chunk: last_pos.chunk_pos,
                        new_chunk: chunk_pos,
                    },
                    entity,
                );

                last_pos.chunk_pos = chunk_pos;
            }
            None => {
                // If this entity doesn't have a LastPosition component we add one and send events
                // with "new" set to true. This indicates to any readers that the events are for entities
                // that were just inserted and didn't have any position history.
                let chunk_pos = transform_chunk_pos(transform);

                cmds.trigger_targets(
                    CrossChunkBorder {
                        new: true,
                        old_chunk: chunk_pos,
                        new_chunk: chunk_pos,
                    },
                    entity,
                );

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

pub fn update_observer_batches(
    trigger: Trigger<CrossChunkBorder>,
    q_observers: Query<(&ObserverBatches, &ObserverSettings)>,
    mut q_batches: Query<&mut ChunkBatch>,
) {
    let observer_entity = trigger.entity();
    let event = trigger.event();
    let (observer_batches, settings) = q_observers.get(observer_entity).unwrap();

    for &batch_entity in observer_batches.owned.iter() {
        let mut batch = q_batches
            .get_mut(batch_entity)
            .expect("Batches should automatically track their own ownership with lifecycle hooks, so if this observer owns this batch, it should exist in the world");

        batch
            .chunks
            .retain(|cpos| settings.within_range(event.new_chunk, *cpos));

        let bb = settings.bounding_box();

        for chunk_pos in bb.cartesian_iter() {
            let chunk_pos = ChunkPos::from(chunk_pos + event.new_chunk.as_ivec3());

            if batch.chunks.contains(chunk_pos) {
                continue;
            }

            batch.chunks.set(chunk_pos);
        }
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
    observers: Query<&Transform, With<ObserverSettings>>,
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
