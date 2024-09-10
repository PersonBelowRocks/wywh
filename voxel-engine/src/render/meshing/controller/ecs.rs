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
    render::lod::LevelOfDetail,
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

use super::{ExtractableChunkMeshData, RemeshPriority, RemeshType};

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
pub fn insert_chunks(
    workers: Res<MeshBuilderPool>,
    mut meshes: ResMut<ExtractableChunkMeshData>,
    realm: VoxelRealm,
) {
    let finished = workers.get_finished_meshes();

    for mesh in finished.into_iter() {
        if !realm.has_render_permit(mesh.pos) {
            continue;
        }

        meshes.add_chunk_mesh(mesh);
    }
}

/// Remove the extracted chunks from the render world when their render permits are revoked
pub fn remove_chunks(
    mut meshes: ResMut<ExtractableChunkMeshData>,
    mut events: EventReader<RemovedBatchChunks>,
    members: Res<CachedBatchMembership>,
    q_batches: Query<&ChunkBatchLod, With<ChunkBatch>>,
    mut builder: ResMut<MeshBuilderPool>,
) {
    let mut remove = ChunkSet::with_capacity(events.len());
    for event in events.read() {
        // Skip if this isn't a renderable batch
        let Some(lod) = q_batches.get(event.batch).ok() else {
            continue;
        };

        for &chunk in event.chunks.iter() {
            if !members.has_flags(chunk, BatchFlags::RENDER) {
                meshes.remove_chunk(chunk, lod.0);
                remove.set(chunk);
            }
        }
    }

    builder.remove_pending(&remove);
}

/// Batches chunks for extraction. Will allow extraction every 500ms (by default).
pub fn batch_chunk_extraction(
    time: Res<Time<Real>>,
    mut meshes: ResMut<ExtractableChunkMeshData>,
    mut last_extract: Local<Option<Instant>>,
) {
    let Some(now) = time.last_update() else {
        return;
    };

    let previous = *last_extract.get_or_insert(now);

    if now - previous > Duration::from_millis(500) {
        *last_extract = Some(now);
        meshes.should_extract = true;
    }
}
