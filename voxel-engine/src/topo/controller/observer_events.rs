use bevy::prelude::*;

use crate::{
    render::{
        lod::LevelOfDetail,
        meshing::controller::events::{BuildChunkMeshEvent, MeshJobUrgency},
    },
    topo::world::{
        chunk_manager::ChunkLoadResult,
        chunk_populator::events::{ChunkPopulated, PopulateChunk},
        ChunkPos,
    },
    util::{closest_distance_sq, ws_to_chunk_pos},
};

use super::{
    AddBatchChunks, ChunkBatch, CrossChunkBorder, LastPosition, LoadChunks, LoadReasons,
    LoadedChunkEvent, ObserverBatches, ObserverLoadshare, ObserverSettings, RemoveBatchChunks,
    UnloadChunks, UpdateCachedChunkFlags, VoxelWorldTick,
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
                auto_populate: true,
                chunks: in_range.clone(),
            });

            cmds.trigger_targets(AddBatchChunks(in_range), batch_entity);
        }
    }

    if !update_cached_flags.is_empty() {
        cmds.trigger(UpdateCachedChunkFlags(update_cached_flags));
    }
}

/// System for dispatching population events for newly loaded chunks.
pub fn populate_loaded_chunks(
    q_observers: Query<&Transform, With<ObserverSettings>>,
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

        populate_chunk_events.send(PopulateChunk {
            chunk_pos: loaded.chunk_pos,
            // Closer chunk positions are higher priority, so we need to invert the distance.
            priority: u32::MAX - (min_distance_sq.ceil() as u32),
        });
    }
}

/// System for dispatching mesh building events for revived chunks.
pub fn build_revived_chunk_meshes(
    q_observers: Query<&Transform, With<ObserverSettings>>,
    mut loaded_chunk_events: EventReader<LoadedChunkEvent>,
    mut mesh_build_events: EventWriter<BuildChunkMeshEvent>,
    tick: Res<VoxelWorldTick>,
) {
    for loaded in loaded_chunk_events.read() {
        if matches!(loaded.load_result, ChunkLoadResult::New) {
            continue;
        }

        let center = loaded.chunk_pos.worldspace_center();
        let observer_positions = q_observers.iter().map(|&transform| transform.translation);
        let min_distance_sq = closest_distance_sq(center, observer_positions).unwrap_or(0.0);

        let priority = u32::MAX - (min_distance_sq.ceil() as u32);

        mesh_build_events.send(BuildChunkMeshEvent {
            chunk_pos: loaded.chunk_pos,
            urgency: MeshJobUrgency::P1(priority),
            lod: LevelOfDetail::X16Subdiv,
            tick: tick.get(),
        });
    }
}

pub fn build_populated_chunk_meshes(
    q_observers: Query<&Transform, With<ObserverSettings>>,
    mut populated_chunk_events: EventReader<ChunkPopulated>,
    mut mesh_build_events: EventWriter<BuildChunkMeshEvent>,
    tick: Res<VoxelWorldTick>,
) {
    for populated in populated_chunk_events.read() {
        let center = populated.chunk_pos.worldspace_center();
        let observer_positions = q_observers.iter().map(|&transform| transform.translation);
        let min_distance_sq = closest_distance_sq(center, observer_positions).unwrap_or(0.0);

        let priority = u32::MAX - (min_distance_sq.ceil() as u32);

        mesh_build_events.send(BuildChunkMeshEvent {
            chunk_pos: populated.chunk_pos,
            urgency: MeshJobUrgency::P1(priority),
            lod: LevelOfDetail::X16Subdiv,
            tick: tick.get(),
        });
    }
}
