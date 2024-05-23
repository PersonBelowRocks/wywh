use std::{cmp::max, time::Duration};

use bevy::{
    prelude::*,
    tasks::{available_parallelism, TaskPool, TaskPoolBuilder},
};

use itertools::Itertools;

use crate::{
    data::{registries::Registries, tile::Face},
    render::meshing::controller::workers::MeshBuilderSettings,
    topo::{
        world::{chunk::ChunkFlags, Chunk, ChunkPos, VoxelRealm},
        ChunkObserver,
    },
    util::ChunkMap,
};

use super::{
    workers::{MeshBuilder, MeshCommand},
    ChunkMeshStatus, ChunkRenderPermit, ChunkRenderPermits, ExtractableChunkMeshData,
    RemeshPriority, RemeshType, TimedChunkMeshData,
};

#[derive(Resource, Deref)]
pub struct MeshWorkerTaskPool(TaskPool);

#[derive(Resource, Default, Deref, DerefMut)]
pub struct MeshGeneration(pub u64);

#[derive(Event, Clone)]
pub struct RemeshChunk {
    pub pos: ChunkPos,
    pub remesh_type: RemeshType,
    pub priority: RemeshPriority,
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
/// This system keeps track of chunk render permits as they are added and removed.
/// Will also dispatch remeshing events as necessary (currently not implemented)
pub fn handle_incoming_permits(
    mut grants: EventReader<GrantPermit>,
    mut revocations: EventReader<RevokePermit>,
    mut permits_r: ResMut<ChunkRenderPermits>,
    mut meshes: ResMut<ExtractableChunkMeshData>,
) {
    let permits = &mut permits_r.permits;

    let mut revoked = 0;
    for event in revocations.read() {
        revoked += 1;
        if let Some(permit) = permits.get(event.pos) {
            if permit.granted < event.generation {
                permits.remove(event.pos);
                meshes.active.remove(event.pos);
                meshes.removed.push(event.pos);
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

/// This system queues meshing jobs in the mesh builder from `RemeshChunk` events.
pub fn queue_chunk_mesh_jobs(
    mut builder: ResMut<MeshBuilder>,
    mut events: EventReader<RemeshChunk>,
    mut current_generation: ResMut<MeshGeneration>,
) {
    if events.len() > 0 {
        current_generation.0 += 1;
        debug!("Queuing {} chunks for remeshing from events", events.len());
    }

    let mut commands = Vec::<MeshCommand>::with_capacity(events.len());
    let mut immediate = Vec::<MeshCommand>::new();

    for event in events.read() {
        let cmd = MeshCommand {
            pos: event.pos,
            priority: event.priority,
            generation: event.generation,
        };

        match event.remesh_type {
            RemeshType::Delayed => commands.push(cmd),
            RemeshType::Immediate => immediate.push(cmd),
        }
    }

    builder.queue_jobs(commands.into_iter());

    for _cmd in immediate.iter() {
        error!("Not yet implemented!");
    }
}

/// This system makes finished chunk meshes available for extraction by the renderer.
pub fn insert_chunks(workers: Res<MeshBuilder>, mut meshes: ResMut<ExtractableChunkMeshData>) {
    let mut total = 0;

    let finished = workers.get_finished_meshes();

    if finished.len() > 0 {
        debug!("Inserting finished chunk meshes");
    }

    let mut insert = ChunkMap::<TimedChunkMeshData>::new();
    for mesh in finished.into_iter() {
        total += 1;

        let Some(existing) = meshes.active.get(mesh.pos) else {
            insert.set(
                mesh.pos,
                TimedChunkMeshData {
                    generation: mesh.generation,
                    data: ChunkMeshStatus::from_mesh_data(&mesh.data),
                },
            );
            continue;
        };

        if existing.generation > mesh.generation {
            continue;
        }

        insert.set(
            mesh.pos,
            TimedChunkMeshData {
                generation: mesh.generation,
                data: ChunkMeshStatus::from_mesh_data(&mesh.data),
            },
        );
    }

    insert.for_each_entry(|pos, chunk_data| {
        meshes.active.set(pos, chunk_data.clone());
    });

    if total > 0 {
        debug!("Inserted {} chunks", total);
    }
}

pub struct UpdateDetectionRemeshResults {
    primary: hb::HashSet<ChunkPos, fxhash::FxBuildHasher>,
    neighbors: hb::HashSet<ChunkPos, fxhash::FxBuildHasher>,
}

/// This system tracks updates in the voxel realm and dispatches remesh events accordingly.
/// Will dispatch remesh events for chunks neighboring the updated chunks if necessary.
pub fn voxel_realm_remesh_updated_chunks(
    time: Res<Time>,
    realm: VoxelRealm,
    permits: Res<ChunkRenderPermits>,
    mut last_queued_fresh: Local<Duration>,
) -> UpdateDetectionRemeshResults {
    let mut remeshings_issued = 0;
    let mut neighbor_remeshings_issued = 0;

    let cm = realm.cm();
    let updated = cm.updated_chunks();

    let current = time.elapsed();
    let time_since_last_fresh_build = current - *last_queued_fresh;

    // Hueristic check to see if it's worth queuing our fresh chunks, we don't want too many to accumulate at a time and we don't want
    // to wait too long between each queuing. These numbers are kind of stupid rn and are just chosen randomly, but maybe a better hueristic can
    // be implemented in the future.
    let should_queue_fresh =
        updated.num_fresh_chunks() > 300 || time_since_last_fresh_build.as_millis() > 1000;

    if should_queue_fresh {
        *last_queued_fresh = current;
    }

    // We need this to keep track of queued chunks, we don't want to queue chunks for remeshing twice!
    let mut queued_primary = hb::HashSet::<ChunkPos, fxhash::FxBuildHasher>::default();
    let mut queued_neighbors = hb::HashSet::<ChunkPos, fxhash::FxBuildHasher>::default();

    updated
        .iter_chunks(|cref| {
            // Don't remesh chunks we don't have a permit to render, and don't remesh already queued chunks.
            if !permits.has_permit(cref.pos()) || queued_primary.contains(&cref.pos()) {
                return;
            }

            // If this chunk was previously queued as a neighbor remesh, we "convert" it to a primary
            // remesh. This is because we need to unflag all chunks that were updated, but we don't want
            // to do that to the neighbors.
            if queued_neighbors.contains(&cref.pos()) {
                queued_neighbors.remove(&cref.pos());
            }

            if cref.flags().contains(ChunkFlags::FRESH) && !should_queue_fresh {
                return;
            }

            queued_primary.insert(cref.pos());
            remeshings_issued += 1;

            // This chunk was updated in such a way that we need to remesh its neighbors too!
            if cref.flags().contains(ChunkFlags::REMESH_NEIGHBORS) {
                for face in Face::FACES {
                    let neighbor_pos = ChunkPos::from(face.normal() + IVec3::from(cref.pos()));

                    if !permits.has_permit(neighbor_pos)
                        || queued_primary.contains(&neighbor_pos)
                        || queued_neighbors.contains(&neighbor_pos)
                    {
                        continue;
                    }

                    // We only remesh the neighbor if it's neither generating or fresh.
                    // We don't mesh generating neighbors because they contain nothing and will be meshed soon anyway,
                    // and we don't mesh fresh chunks because they'll also be meshed soon anyway.
                    let avoid_flags: ChunkFlags = ChunkFlags::GENERATING | ChunkFlags::FRESH;
                    let Some(flags) = cm.chunk_flags(neighbor_pos) else {
                        continue;
                    };

                    if flags.intersects(avoid_flags) {
                        continue;
                    }

                    queued_neighbors.insert(neighbor_pos);
                    neighbor_remeshings_issued += 1;
                }
            }
        })
        .unwrap();

    for pos in &queued_primary {
        if let Ok(cref) = cm.get_loaded_chunk(*pos) {
            cref.update_flags(|flags| {
                flags.remove(ChunkFlags::REMESH | ChunkFlags::REMESH_NEIGHBORS | ChunkFlags::FRESH)
            });
        }
    }

    let span = debug_span!("auto-remesh-report");
    span.in_scope(|| {
        if remeshings_issued > 0 || neighbor_remeshings_issued > 0 {
            let total_remeshings = remeshings_issued + neighbor_remeshings_issued;
            debug!("Primary remeshes: {remeshings_issued}");
            debug!("Neighbor remeshes: {neighbor_remeshings_issued}");
            debug!("Total remeshes: {total_remeshings}");
        }
    });

    UpdateDetectionRemeshResults {
        primary: queued_primary,
        neighbors: queued_neighbors,
    }
}

fn calculate_priority(trans: &Transform, chunk_pos: ChunkPos) -> RemeshPriority {
    const CHUNK_SIZE_F32: f32 = Chunk::SIZE as f32;
    const CHUNK_SIZE_DIV2: f32 = CHUNK_SIZE_F32 / 2.0;

    let chunk_center = (chunk_pos.as_vec3() * CHUNK_SIZE_F32) + Vec3::splat(CHUNK_SIZE_DIV2);

    let distance_sq = chunk_center.distance_squared(trans.translation);
    let distance_sq_int = distance_sq.clamp(0.0, u32::MAX as _) as u32;

    RemeshPriority::new(distance_sq_int)
}

/// This system dispatches remesh jobs for chunks discovered by `voxel_realm_remesh_updated_chunks`
pub fn dispatch_updated_chunk_remeshings(
    In(detected): In<UpdateDetectionRemeshResults>,
    current_generation: Res<MeshGeneration>,
    observers: Query<&Transform, With<ChunkObserver>>,
    mut writer: EventWriter<RemeshChunk>,
) {
    writer.send_batch(
        detected
            .primary
            .into_iter()
            .chain(detected.neighbors.into_iter())
            .map(|chunk_pos| {
                // Calculate remesh priority based on distance to nearest "observer"
                let priority = observers
                    .iter()
                    .map(|trans| calculate_priority(trans, chunk_pos))
                    .max()
                    .unwrap_or(RemeshPriority::LOWEST);

                RemeshChunk {
                    pos: chunk_pos,
                    remesh_type: RemeshType::Delayed,
                    priority,
                    generation: current_generation.0,
                }
            }),
    );
}

/// Sets up the background mesh builder pool.
pub fn setup_chunk_meshing_workers(
    mut cmds: Commands,
    registries: Res<Registries>,
    realm: VoxelRealm,
) {
    info!("Setting up chunk meshing workers");

    let task_pool = TaskPoolBuilder::new()
        .thread_name("Mesh Worker Task Pool".into())
        .num_threads(max(1, available_parallelism() / 2))
        .build();

    let settings = MeshBuilderSettings {
        workers: task_pool.thread_num(),
        job_channel_capacity: task_pool.thread_num() * 4,
        worker_mesh_backlog_capacity: 3,
    };

    let worker_pool = MeshBuilder::new(settings, &task_pool, registries.clone(), realm.clone_cm());

    cmds.insert_resource(worker_pool);
    cmds.insert_resource(MeshWorkerTaskPool(task_pool));
}
