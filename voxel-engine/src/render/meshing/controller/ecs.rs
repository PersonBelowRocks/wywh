use std::{
    cmp::max,
    time::{Duration, Instant},
};

use bevy::{
    prelude::*,
    tasks::{available_parallelism, TaskPool, TaskPoolBuilder},
};

use crate::{
    data::{registries::Registries, tile::Face},
    render::lod::{LODs, LevelOfDetail},
    topo::{
        controller::{
            BatchFlags, CachedBatchMembership, ChunkBatch, ChunkBatchLod, RemovedBatchChunks,
            VoxelWorldTick,
        },
        world::{chunk::ChunkFlags, Chunk, ChunkPos, VoxelRealm},
        ObserverSettings,
    },
    util::{sync::LockStrategy, ChunkSet},
};

use super::{
    events::{MeshFinishedEvent, RemoveChunkMeshEvent},
    ChunkMeshExtractBridge, RemeshPriority, RemeshType,
};

#[derive(Resource, Deref)]
pub struct MeshWorkerTaskPool(TaskPool);

/// Chunks that act as occluders for HZB occlusion culling. Packaged into a vector
/// for easy extraction to the render world.
#[derive(Resource, Deref, Default)]
pub struct OccluderChunks(Vec<ChunkPos>);

#[derive(Event, Clone)]
pub struct RemeshChunk {
    pub pos: ChunkPos,
    pub lod: LevelOfDetail,
    pub remesh_type: RemeshType,
    pub priority: RemeshPriority,
    pub tick: u64,
}

pub fn collect_solid_chunks_as_occluders(realm: VoxelRealm, mut occluders: ResMut<OccluderChunks>) {
    occluders.0 = realm.cm().solid_chunks();
}

/// This system makes finished chunk meshes available for extraction by the renderer.
pub fn prepare_finished_meshes_for_extraction(
    mut finished: EventReader<MeshFinishedEvent>,
    mut bridge: ResMut<ChunkMeshExtractBridge>,
    realm: VoxelRealm,
) {
    for event in finished.read() {
        if !realm.has_render_permit(event.chunk_pos) {
            continue;
        }

        bridge.add_chunk_mesh(event.chunk_pos, event.lod, event.tick, event.mesh.clone());
    }
}

/// Send [`RemoveChunkMeshEvent`]s when chunks are removed from batches.
pub fn send_mesh_removal_events_from_batch_removal_events(
    mut events: EventReader<RemovedBatchChunks>,
    members: Res<CachedBatchMembership>,
    q_batches: Query<&ChunkBatchLod, With<ChunkBatch>>,
    mut removal_events: EventWriter<RemoveChunkMeshEvent>,
    tick: Res<VoxelWorldTick>,
) {
    for event in events.read() {
        // Skip if this isn't a renderable batch
        let Some(lod) = q_batches.get(event.batch).ok() else {
            continue;
        };

        for &chunk_pos in event.chunks.iter() {
            if !members.has_flags(chunk_pos, BatchFlags::RENDER) {
                // TODO: handle these events
                removal_events.send(RemoveChunkMeshEvent {
                    chunk_pos,
                    lods: lod.bitflag(),
                    tick: tick.get(),
                });
            }
        }
    }
}

/// Removes chunk meshes from [`ChunkMeshExtractBridge`] based on removal events.
pub fn remove_chunk_meshes_from_extraction_bridge(
    mut removal_events: EventReader<RemoveChunkMeshEvent>,
    mut bridge: ResMut<ChunkMeshExtractBridge>,
) {
    for event in removal_events.read() {
        for lod in event.lods.contained_lods() {
            bridge.remove_chunk(event.chunk_pos, lod, event.tick);
        }
    }
}

/// Batches chunks for extraction. Will allow extraction every 500ms (by default).
pub fn batch_chunk_extraction(
    time: Res<Time<Real>>,
    mut bridge: ResMut<ChunkMeshExtractBridge>,
    mut last_extract: Local<Option<Instant>>,
) {
    let Some(now) = time.last_update() else {
        return;
    };

    let previous = *last_extract.get_or_insert(now);

    if now - previous > Duration::from_millis(500) {
        *last_extract = Some(now);
        bridge.should_extract = true;
    }
}
