use std::{sync::Arc, thread::JoinHandle};

use async_bevy_events::{AsyncEventReader, EventFunnel};
use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
};
use dashmap::mapref::entry::Entry;
use flume::{Receiver, Sender};
use itertools::Itertools;

use parking_lot::lock_api::RwLockUpgradableReadGuard as ReadGuard;

use crate::{
    topo::{
        controller::{
            LoadChunks, LoadReasons, LoadReasonsAddedEvent, LoadReasonsRemovedEvent,
            LoadedChunkEvent, LoadshareId, PurgedChunkEvent, UnloadChunks,
        },
        world::{
            chunk::ChunkFlags,
            chunk_manager::{inner_storage::LoadedChunk, ChunkLoadshares},
            Chunk, ChunkPos,
        },
    },
    util::{sync::LockStrategy, ChunkSet},
};

use super::{ChunkManager, ChunkStorageStructure};

/// The granularity of the lock in asynchronous chunk lifecycle tasks.
/// Performance may be different with different values.
/// Too high or too low values can be slow and janky, but *should (hopefully)* never cause any actual bugs other than
/// performance issues.
#[derive(Resource, Clone)]
pub struct ChunkLifecycleTaskLockGranularity(pub usize);

/// An ECS resource for the chunk manager.
#[derive(Resource, Deref)]
pub struct ChunkManagerRes(pub Arc<ChunkManager>);

fn handle_load_chunk<'a>(
    access: &ChunkStorageStructure<'a>,
    chunk_manager: &ChunkManager,
    loaded_chunk_funnel: &EventFunnel<LoadedChunkEvent>,
    added_load_reasons_funnel: &EventFunnel<LoadReasonsAddedEvent>,
    chunk_pos: ChunkPos,
    load_reasons: LoadReasons,
    loadshare: LoadshareId,
    auto_generate: bool,
) {
    if access.is_loaded(chunk_pos) {
        access
            .add_loadshare_load_reasons(chunk_pos, loadshare, load_reasons)
            .unwrap();

        added_load_reasons_funnel
            .send(LoadReasonsAddedEvent {
                chunk_pos,
                reasons_added: load_reasons,
                loadshare: loadshare,
                was_loaded: false,
            })
            .unwrap();
    } else {
        let result = access
            .load_chunk(chunk_manager.new_primordial_chunk(chunk_pos))
            .unwrap();
        // Load reasons must be manually added
        access
            .add_loadshare_load_reasons(chunk_pos, loadshare, load_reasons)
            .unwrap();

        added_load_reasons_funnel
            .send(LoadReasonsAddedEvent {
                chunk_pos,
                reasons_added: load_reasons,
                loadshare: loadshare,
                // These load reasons caused the chunk to be loaded
                was_loaded: true,
            })
            .unwrap();

        loaded_chunk_funnel
            .send(LoadedChunkEvent {
                chunk_pos,
                auto_generate: auto_generate,
                load_result: result,
            })
            .unwrap();
    }
}

pub fn start_async_chunk_load_task(
    load_chunks: Res<AsyncEventReader<LoadChunks>>,
    loaded_chunk_funnel: Res<EventFunnel<LoadedChunkEvent>>,
    added_load_reasons_funnel: Res<EventFunnel<LoadReasonsAddedEvent>>,
    chunk_manager: Res<ChunkManagerRes>,
    lock_granularity: Res<ChunkLifecycleTaskLockGranularity>,
) {
    let lock_granularity = lock_granularity.0;
    let chunk_manager = chunk_manager.clone();

    let load_chunks = load_chunks.clone();
    let loaded_chunk_funnel = loaded_chunk_funnel.clone();
    let added_load_reasons_funnel = added_load_reasons_funnel.clone();

    info!("Starting asynchronous chunk LOADING task. GRANULARITY={lock_granularity}");

    AsyncComputeTaskPool::get()
        .spawn(async move {
            while let Ok(mut event) = load_chunks.recv_async().await {
                // Don't do anything if there are no reasons. Chunks should never be loaded without any load reasons!
                if event.reasons.is_empty() {
                    return;
                }

                while !event.chunks.is_empty() {
                    chunk_manager
                        .structural_access(LockStrategy::Blocking, |access| {
                            // We're coarsely locking here to give other tasks a chance to make changes to the chunk storage.
                            for _ in 0..lock_granularity {
                                let Some(chunk_pos) = event.chunks.pop() else {
                                    break;
                                };

                                handle_load_chunk(
                                    &access,
                                    chunk_manager.as_ref(),
                                    &loaded_chunk_funnel,
                                    &added_load_reasons_funnel,
                                    chunk_pos,
                                    event.reasons,
                                    event.loadshare,
                                    event.auto_generate,
                                );
                            }
                        })
                        .unwrap();
                }
            }
        })
        .detach();
}

fn handle_purge_chunk<'a>(
    access: &ChunkStorageStructure<'a>,
    purged_chunk_funnel: &EventFunnel<PurgedChunkEvent>,
    removed_load_reasons_funnel: &EventFunnel<LoadReasonsRemovedEvent>,
    chunk_pos: ChunkPos,
    remove_reasons: LoadReasons,
    loadshare: LoadshareId,
) {
    if access
        .remove_loadshare_load_reasons(chunk_pos, loadshare, remove_reasons)
        .is_err()
    {
        return;
    }

    let Ok(load_reasons_union) = access.load_reasons_union(chunk_pos) else {
        return;
    };

    let mut was_purged = false;
    if load_reasons_union.is_empty() {
        access.purge_chunk(chunk_pos).unwrap();

        purged_chunk_funnel
            .send(PurgedChunkEvent { chunk_pos })
            .unwrap();
        was_purged = true;
    }

    removed_load_reasons_funnel
        .send(LoadReasonsRemovedEvent {
            chunk_pos,
            reasons_removed: remove_reasons,
            loadshare,
            was_purged,
        })
        .unwrap();
}

pub fn start_async_chunk_purge_task(
    unload_chunks: Res<AsyncEventReader<UnloadChunks>>,
    purged_chunk_funnel: Res<EventFunnel<PurgedChunkEvent>>,
    removed_load_reasons_funnel: Res<EventFunnel<LoadReasonsRemovedEvent>>,
    chunk_manager: Res<ChunkManagerRes>,
    lock_granularity: Res<ChunkLifecycleTaskLockGranularity>,
) {
    let lock_granularity = lock_granularity.0;
    let chunk_manager = chunk_manager.clone();

    let unload_chunks = unload_chunks.clone();
    let purged_chunk_funnel = purged_chunk_funnel.clone();
    let removed_load_reasons_funnel = removed_load_reasons_funnel.clone();

    info!("Starting asynchronous chunk PURGING task. GRANULARITY={lock_granularity}");

    AsyncComputeTaskPool::get()
        .spawn(async move {
            while let Ok(mut event) = unload_chunks.recv_async().await {
                // Don't do anything if there are no reasons. Chunks should never be loaded without any load reasons!
                if event.reasons.is_empty() {
                    return;
                }

                while !event.chunks.is_empty() {
                    chunk_manager
                        .structural_access(LockStrategy::Blocking, |access| {
                            // We're coarsely locking here to give other tasks a chance to make changes to the chunk storage.
                            for _ in 0..lock_granularity {
                                let Some(chunk_pos) = event.chunks.pop() else {
                                    break;
                                };

                                handle_purge_chunk(
                                    &access,
                                    &purged_chunk_funnel,
                                    &removed_load_reasons_funnel,
                                    chunk_pos,
                                    event.reasons,
                                    event.loadshare,
                                );
                            }
                        })
                        .unwrap();
                }
            }
        })
        .detach();
}
