use bevy::prelude::*;

use crate::{
    topo::world::{Chunk, ChunkPos},
    util::{ws_to_chunk_pos, ChunkSet},
};

use super::{
    AddBatchChunks, ChunkBatch, CrossChunkBorder, LastPosition, LoadChunks, LoadReasons,
    LoadedChunkEvent, ObserverBatches, ObserverLoadshare, ObserverSettings, RemoveBatchChunks,
    UnloadChunks, UpdateCachedChunkFlags,
};

fn transform_chunk_pos(trans: &Transform) -> ChunkPos {
    ws_to_chunk_pos(trans.translation.floor().as_ivec3())
}

pub fn dispatch_move_events(
    mut observers: Query<(Entity, &Transform, Option<&mut LastPosition>), With<ObserverSettings>>,
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
    q_observers: Query<(
        Option<&ObserverBatches>,
        Option<&ObserverLoadshare>,
        &ObserverSettings,
    )>,
    q_batches: Query<&mut ChunkBatch>,
    mut load_chunks: EventWriter<LoadChunks>,
    mut unload_chunks: EventWriter<UnloadChunks>,
    mut cmds: Commands,
) {
    let observer_entity = trigger.entity();
    let event = trigger.event();
    let (observer_batches, loadshare, settings) = q_observers.get(observer_entity).unwrap();

    let Some(observer_batches) = observer_batches else {
        // This observer doesn't have any batches
        return;
    };

    let Some(loadshare) = loadshare else {
        return;
    };

    let loadshare_id = loadshare.get()
        .expect("We shouldn't be able to produce an observer loadshare component that doesn't have an ID yet 
            since we're using component hooks to set it immediately");

    let mut update_cached_flags = Vec::with_capacity(64);

    for &batch_entity in observer_batches.owned.iter() {
        let batch = q_batches
            .get(batch_entity)
            .expect("Batches should automatically track their own ownership with lifecycle hooks, so if this observer owns this batch, it should exist in the world");

        let mut out_of_range = Vec::with_capacity(64);

        // Remove out of range chunks
        out_of_range.extend(
            batch
                .chunks()
                .iter()
                .filter(|&c| !settings.within_range(event.new_chunk, c))
                .inspect(|&c| update_cached_flags.push(c)),
        );

        if !out_of_range.is_empty() {
            unload_chunks.send(UnloadChunks {
                loadshare: loadshare_id,
                reasons: LoadReasons::RENDER,
                chunks: out_of_range.clone(),
            });

            cmds.trigger_targets(RemoveBatchChunks(out_of_range), batch_entity);
        }

        // Add in-range chunks
        let mut in_range = Vec::with_capacity(64);

        in_range.extend(
            settings
                .bounding_box()
                .cartesian_iter()
                .map(|pos| pos + event.new_chunk.as_ivec3())
                .map(ChunkPos::from)
                .filter(|&cpos| !batch.chunks().contains(cpos))
                .inspect(|&c| update_cached_flags.push(c)),
        );

        if !in_range.is_empty() {
            load_chunks.send(LoadChunks {
                loadshare: loadshare_id,
                reasons: LoadReasons::RENDER,
                auto_generate: true,
                chunks: in_range.clone(),
            });

            cmds.trigger_targets(AddBatchChunks(in_range), batch_entity);
        }
    }

    if !update_cached_flags.is_empty() {
        cmds.trigger(UpdateCachedChunkFlags(update_cached_flags));
    }
}
