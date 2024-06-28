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
    render::meshing::controller::workers::MeshBuilderSettings,
    topo::{
        controller::{BatchFlags, PermitLostFlagsEvent, VoxelWorldTick},
        world::{chunk::ChunkFlags, Chunk, ChunkPos, VoxelRealm},
        ObserverSettings,
    },
    util::ChunkSet,
};

use super::{
    workers::{MeshBuilder, MeshBuilderCommand},
    ChunkMeshStatus, ExtractableChunkMeshData, RemeshPriority, RemeshType, TimedChunkMeshStatus,
};

#[derive(Resource, Deref)]
pub struct MeshWorkerTaskPool(TaskPool);

#[derive(Event, Clone)]
pub struct RemeshChunk {
    pub pos: ChunkPos,
    pub remesh_type: RemeshType,
    pub priority: RemeshPriority,
    pub tick: u64,
}

/// This system queues meshing jobs in the mesh builder from `RemeshChunk` events.
pub fn queue_chunk_mesh_jobs(
    mut builder: ResMut<MeshBuilder>,
    mut events: EventReader<RemeshChunk>,
) {
    if events.len() > 0 {
        debug!("Queuing {} chunks for remeshing from events", events.len());
    }

    let mut commands = Vec::<MeshBuilderCommand>::with_capacity(events.len());
    let mut immediate = Vec::<MeshBuilderCommand>::new();

    for event in events.read() {
        let cmd = MeshBuilderCommand {
            pos: event.pos,
            priority: event.priority,
            generation: event.tick,
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
pub fn insert_chunks(
    workers: Res<MeshBuilder>,
    mut meshes: ResMut<ExtractableChunkMeshData>,
    realm: VoxelRealm,
) {
    let finished = workers.get_finished_meshes();

    let ExtractableChunkMeshData {
        active,
        remove,
        should_extract: _,
        added,
    } = meshes.as_mut();

    for mesh in finished.into_iter() {
        if !realm.has_render_permit(mesh.pos) {
            continue;
        }

        let existing: &mut TimedChunkMeshStatus =
            active.entry(mesh.pos).or_insert(TimedChunkMeshStatus {
                generation: mesh.generation,
                status: ChunkMeshStatus::from_mesh_data(&mesh.data),
            });

        // If the existing chunk's generation is less than or equal to the new chunk's generation, then we
        // want to update both the status and the generation, and also add the chunk mesh to the extract data.
        // This will be done both if the existing chunk is older than the new one, or if we just added (i.e., generation is the same)
        // a chunk. If we just added the chunk we also want to run this logic because we want to extract its mesh data.
        if existing.generation <= mesh.generation {
            existing.generation = mesh.generation;
            existing.status = ChunkMeshStatus::from_mesh_data(&mesh.data);

            match existing.status {
                // If the chunk mesh is empty, we remove it from the render world
                ChunkMeshStatus::Empty => {
                    remove.set(mesh.pos);
                }
                // If it's filled, extract it
                ChunkMeshStatus::Filled => {
                    if realm.has_render_permit(mesh.pos) {
                        added.set(mesh.pos, mesh.data);
                    }
                }
                // We just updated this field and know it can't be any of these values
                ChunkMeshStatus::Unfulfilled | ChunkMeshStatus::Extracted => unreachable!(),
            }
        }
    }
}

/// Remove the extracted chunks from the render world when their render permits are revoked
pub fn remove_chunks(
    mut meshes: ResMut<ExtractableChunkMeshData>,
    mut events: EventReader<PermitLostFlagsEvent>,
    mut builder: ResMut<MeshBuilder>,
) {
    let mut remove = ChunkSet::with_capacity(events.len());
    for event in events.read() {
        if event.lost_flags.contains(BatchFlags::RENDER) {
            remove.set(event.chunk_pos);
        }
    }

    builder.remove_pending(&remove);
    for chunk in remove.iter() {
        meshes.remove.set(chunk);
        meshes.active.remove(chunk);
    }
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

pub struct UpdateDetectionRemeshResults {
    primary: hb::HashSet<ChunkPos, fxhash::FxBuildHasher>,
    neighbors: hb::HashSet<ChunkPos, fxhash::FxBuildHasher>,
}

/// This system tracks updates in the voxel realm and dispatches remesh events accordingly.
/// Will dispatch remesh events for chunks neighboring the updated chunks if necessary.
pub fn voxel_realm_remesh_updated_chunks(
    time: Res<Time>,
    realm: VoxelRealm,
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

    // TODO: skip this update if the chunk manager is globally locked.
    let result = updated.iter_chunks(|cref| {
        // Don't remesh chunks we don't have a permit to render, and don't remesh already queued chunks.
        if !realm.has_render_permit(cref.pos()) || queued_primary.contains(&cref.pos()) {
            return;
        }

        // If this chunk was previously queued as a neighbor remesh, we "convert" it to a primary
        // remesh. This is because we need to unflag all chunks that were updated, but we don't want
        // to do that to the neighbors.
        if queued_neighbors.contains(&cref.pos()) {
            queued_neighbors.remove(&cref.pos());
        }

        if cref.flags().contains(ChunkFlags::FRESHLY_GENERATED) && !should_queue_fresh {
            return;
        }

        queued_primary.insert(cref.pos());
        remeshings_issued += 1;

        // This chunk was updated in such a way that we need to remesh its neighbors too!
        if cref.flags().contains(ChunkFlags::REMESH_NEIGHBORS) {
            for face in Face::FACES {
                let neighbor_pos = ChunkPos::from(face.normal() + IVec3::from(cref.pos()));

                if !realm.has_render_permit(neighbor_pos)
                    || queued_primary.contains(&neighbor_pos)
                    || queued_neighbors.contains(&neighbor_pos)
                {
                    continue;
                }

                // We only remesh the neighbor if it's neither generating or fresh (or primordial).
                // We don't mesh generating neighbors because they contain nothing and will be meshed soon anyway,
                // and we don't mesh fresh chunks because they'll also be meshed soon anyway.
                let avoid_flags: ChunkFlags =
                    ChunkFlags::PRIMORDIAL | ChunkFlags::GENERATING | ChunkFlags::FRESHLY_GENERATED;
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
    });

    if let Err(error) = result {
        // A global lock error is fine (and expected from time to time), we just retry on the next
        // iteration of this system. Since chunks are unflagged in this system, and we didn't manage to
        // queue any chunks for remeshing and unflagging, the flags will be the same on the next iteration.
        if !error.is_globally_locked() {
            error!("Error trying to iterate over remeshable chunks: {error}");
        }
    }

    for pos in &queued_primary {
        if let Ok(cref) = cm.get_loaded_chunk(*pos, false) {
            cref.update_flags(|flags| {
                flags.remove(
                    ChunkFlags::REMESH
                        | ChunkFlags::REMESH_NEIGHBORS
                        | ChunkFlags::FRESHLY_GENERATED,
                )
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
    tick: Res<VoxelWorldTick>,
    observers: Query<&Transform, With<ObserverSettings>>,
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
                    tick: tick.get(),
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
        .num_threads(max(1, available_parallelism() / 4))
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
