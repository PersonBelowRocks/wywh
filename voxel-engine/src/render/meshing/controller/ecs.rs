use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{
    prelude::*,
    render::extract_resource::ExtractResource,
    tasks::{TaskPool, TaskPoolBuilder},
};
use dashmap::DashSet;

use crate::{
    data::{registries::Registries, tile::Face},
    render::meshing::greedy::algorithm::GreedyMesher,
    topo::world::{chunk::ChunkFlags, ChunkPos, VoxelRealm},
    util::{ChunkMap, SyncChunkMap},
};

use super::{workers::MeshWorkerPool, ChunkMeshData, ChunkRenderPermits, TimedChunkMeshData};

#[derive(Resource, Deref)]
pub struct MeshWorkerTaskPool(TaskPool);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct MeshGeneration(u64);

#[derive(Resource, ExtractResource, Deref, Default, Clone)]
pub struct ChunkMeshStorage(Arc<SyncChunkMap<TimedChunkMeshData>>);

#[derive(Event, Clone)]
pub struct RemeshChunk {
    pub pos: ChunkPos,
    pub generation: u64,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct ChunkRenderPermit {
    lod: (), // TODO: LODs!
}

pub fn queue_chunk_mesh_jobs(workers: Res<MeshWorkerPool>, mut events: EventReader<RemeshChunk>) {
    for event in events.read() {
        workers.queue_job(event.pos, event.generation);
    }
}

pub fn insert_chunks(workers: Res<MeshWorkerPool>, meshes: Res<ChunkMeshStorage>) {
    let finished = workers.get_finished_meshes();
    for mesh in finished.into_iter() {
        let existing = meshes.get(mesh.pos);

        match existing {
            Some(existing) if existing.generation < mesh.generation => {
                meshes.set(
                    mesh.pos,
                    TimedChunkMeshData {
                        generation: mesh.generation,
                        data: mesh.data,
                    },
                );
            }

            None => {
                meshes.set(
                    mesh.pos,
                    TimedChunkMeshData {
                        generation: mesh.generation,
                        data: mesh.data,
                    },
                );
            }

            _ => (),
        }
    }
}

pub fn remesh_chunks(
    time: Res<Time>,
    realm: Res<VoxelRealm>,
    permits: Res<ChunkRenderPermits>,
    mut writer: EventWriter<RemeshChunk>,
    mut current_generation: ResMut<MeshGeneration>,
    mut last_queued_fresh: Local<Duration>,
) {
    let cm = realm.chunk_manager.as_ref();

    let updated = cm.updated_chunks();

    let current = time.elapsed();
    let time_since_last_fresh_build = current - *last_queued_fresh;

    // Hueristic check to see if it's worth queuing our fresh chunks, we don't want too many to accumulate at a time and we don't want
    // to wait too long between each queuing. These numbers are kind of stupid rn and are just chosen randomly, but maybe a better hueristic can
    // be implemented in the future.
    let should_queue_fresh =
        updated.num_fresh_chunks() > 100 || time_since_last_fresh_build.as_millis() > 500;

    if should_queue_fresh {
        *last_queued_fresh = current;
    }

    let mut did_queue = false;

    // We need this to keep track of queued chunks, we don't want to queue chunks for remeshing twice!
    let mut queued = hb::HashSet::<ChunkPos, fxhash::FxBuildHasher>::default();

    updated
        .iter_chunks(|cref| {
            // Don't remesh chunks we don't have a permit to render, and don't remesh already queued chunks.
            if permits.has_permit(cref.pos()) && !queued.contains(&cref.pos()) {
                if cref.flags().contains(ChunkFlags::FRESH) && !should_queue_fresh {
                    return;
                }

                writer.send(RemeshChunk {
                    pos: cref.pos(),
                    generation: **current_generation,
                });
                queued.insert(cref.pos());

                // This chunk was updated in such a way that we need to remesh its neighbors too!
                if cref.flags().contains(ChunkFlags::REMESH_NEIGHBORS) {
                    for face in Face::FACES {
                        let neighbor_pos = ChunkPos::from(face.normal() + IVec3::from(cref.pos()));

                        // We only remesh the neighbor if it's neither generating or fresh.
                        // We don't mesh generating neighbors because they contain nothing and will be meshed soon anyway,
                        // and we don't mesh fresh chunks because they'll also be meshed soon anyway.
                        if permits.has_permit(neighbor_pos)
                        && cm.chunk_flags(neighbor_pos).is_some_and(|flags| {
                            !flags.contains(ChunkFlags::GENERATING | ChunkFlags::FRESH)
                        })
                        // We also shouldn't remesh already queued neighboring chunks.
                        && !queued.contains(&neighbor_pos)
                        {
                            writer.send(RemeshChunk {
                                pos: neighbor_pos,
                                generation: **current_generation,
                            });
                            queued.insert(neighbor_pos);
                        }
                    }
                }

                did_queue = true;
            }
        })
        .unwrap();

    // We only update our generation if we actually queued any chunks this run.
    if did_queue {
        current_generation.0 += 1;
    }
}

pub fn setup_chunk_meshing_workers(
    mut cmds: Commands,
    registries: Res<Registries>,
    realm: Res<VoxelRealm>,
) {
    let mesher = GreedyMesher::new();

    let task_pool = TaskPoolBuilder::new()
        .thread_name("Mesh Worker Task Pool".into())
        .build();

    let worker_pool = MeshWorkerPool::new(
        task_pool.thread_num(),
        &task_pool,
        mesher.clone(),
        registries.clone(),
        realm.chunk_manager.clone(),
    );

    cmds.insert_resource(worker_pool);
    cmds.insert_resource(MeshWorkerTaskPool(task_pool));
}
