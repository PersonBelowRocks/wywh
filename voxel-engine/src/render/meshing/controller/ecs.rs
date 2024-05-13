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

use super::{
    workers::MeshWorkerPool, ChunkMeshData, ChunkMeshStatus, ChunkRenderPermit, ChunkRenderPermits,
    TimedChunkMeshData,
};

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

#[derive(Event, Clone)]
pub struct GrantPermit {
    pub pos: ChunkPos,
    pub generation: u64,
}

#[derive(Event, Clone)]
pub struct RevokePermit {
    pub pos: ChunkPos,
    pub generation: u64,
}

// TODO: queue new permits for remeshing
pub fn handle_incoming_permits(
    mut grants: EventReader<GrantPermit>,
    mut revocations: EventReader<RevokePermit>,
    mut permits_r: ResMut<ChunkRenderPermits>,
) {
    let permits = &mut permits_r.permits;

    let mut revoked = 0;
    for event in revocations.read() {
        revoked += 1;
        if let Some(permit) = permits.get(event.pos) {
            if permit.granted < event.generation {
                permits.remove(event.pos);
            }
        }
    }

    let mut granted = 0;
    for event in grants.read() {
        granted += 1;
        let existing = permits.get(event.pos);

        if existing.is_none() || existing.is_some_and(|e| e.granted < event.generation) {
            permits.set(event.pos, ChunkRenderPermit::new(event.generation));
        }
    }

    if revoked > 0 || granted > 0 {
        debug!(
            "Handled {} revoked permits, and {} granted permits.",
            revoked, granted
        );
        debug!("Total active permits is now: {}", permits.len());
    }
}

pub fn queue_chunk_mesh_jobs(workers: Res<MeshWorkerPool>, mut events: EventReader<RemeshChunk>) {
    let mut total = 0;
    for event in events.read() {
        total += 1;
        workers.queue_job(event.pos, event.generation);
    }

    if total > 0 {
        debug!("Queued {} chunks for remeshing from events", total);
    }
}

pub fn insert_chunks(workers: Res<MeshWorkerPool>, meshes: Res<ChunkMeshStorage>) {
    let mut total = 0;

    let finished = workers.get_finished_meshes();

    if finished.len() > 0 {
        debug!("Inserting finished chunk meshes");
    }

    let mut insert = ChunkMap::<TimedChunkMeshData>::new();
    for mesh in finished.into_iter() {
        total += 1;
        let existing = meshes.get(mesh.pos);

        match existing {
            Some(existing) if existing.generation < mesh.generation => {
                insert.set(
                    mesh.pos,
                    TimedChunkMeshData {
                        generation: mesh.generation,
                        data: ChunkMeshStatus::Filled(mesh.data),
                    },
                );
            }

            None => {
                insert.set(
                    mesh.pos,
                    TimedChunkMeshData {
                        generation: mesh.generation,
                        data: ChunkMeshStatus::Filled(mesh.data),
                    },
                );
            }

            _ => (),
        }
    }

    insert.for_each_entry(|pos, chunk_data| {
        meshes.set(pos, chunk_data.clone());
    });

    if total > 0 {
        debug!("Inserted {} chunks", total);
    }
}

pub fn voxel_realm_remesh_updated_chunks(
    time: Res<Time>,
    realm: Res<VoxelRealm>,
    permits: Res<ChunkRenderPermits>,
    mut writer: EventWriter<RemeshChunk>,
    mut current_generation: ResMut<MeshGeneration>,
    mut last_queued_fresh: Local<Duration>,
) {
    let mut remeshings_issued = 0;
    let mut neighbor_remeshings_issued = 0;

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

                remeshings_issued += 1;

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

                            neighbor_remeshings_issued += 1;
                        }
                    }
                }

                did_queue = true;
            }
        })
        .unwrap();

    for pos in queued {
        let Ok(cref) = cm.get_loaded_chunk(pos) else {
            continue;
        };

        cref.update_flags(|flags| {
            flags.remove(ChunkFlags::REMESH | ChunkFlags::REMESH_NEIGHBORS | ChunkFlags::FRESH)
        })
    }

    if remeshings_issued > 0 || neighbor_remeshings_issued > 0 {
        debug!("===Remeshing report===");
        debug!(
            "Issued {} primary remeshing events based on voxel realm updates",
            remeshings_issued
        );
        debug!(
            "Issued {} remeshing events for relevant neighbors of primary remeshing events",
            neighbor_remeshings_issued
        );
        debug!(
            "Total issued remeshing events: {}",
            remeshings_issued + neighbor_remeshings_issued
        );
        debug!("Current mesh generation: {}", current_generation.0);
        debug!("======================\n")
    }

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
    debug!("Setting up chunk meshing workers");

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
