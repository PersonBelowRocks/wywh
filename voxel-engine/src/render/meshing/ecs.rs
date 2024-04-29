use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bevy::math::ivec3;
use bevy::prelude::*;
use bevy::tasks::{TaskPool, TaskPoolBuilder};
use indexmap::IndexSet;

use crate::topo::world::chunk::ChunkFlags;
use crate::{
    data::registries::Registries,
    topo::world::{Chunk, ChunkEntity, ChunkPos, VoxelRealm},
};

use super::greedy::algorithm::GreedyMesher;
use super::MeshWorkerPool;

#[derive(Component)]
pub struct ShouldExtract(AtomicBool);

impl ShouldExtract {
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn set(&self, value: bool) {
        self.0.store(value, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

impl Default for ShouldExtract {
    fn default() -> Self {
        Self(AtomicBool::new(true))
    }
}

#[derive(Resource, Deref)]
pub struct MeshWorkerTaskPool(TaskPool);

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

pub fn queue_chunk_meshing_tasks(
    time: Res<Time>,
    mut last_queued_fresh_chunks: Local<Duration>,
    mut cmds: Commands,
    chunks: Query<&ChunkPos, With<ChunkEntity>>,
    realm: Res<VoxelRealm>,
    workers: Res<MeshWorkerPool>,
) {
    let changes = realm.chunk_manager.updated_chunks();
    let mut queued =
        IndexSet::<ChunkPos, ahash::RandomState>::with_hasher(ahash::RandomState::new());
    let mut extra_queued =
        IndexSet::<ChunkPos, ahash::RandomState>::with_hasher(ahash::RandomState::new());

    let current = time.elapsed();
    let time_since_last_fresh_build = current - *last_queued_fresh_chunks;

    let should_queue_fresh =
        changes.num_fresh_chunks() > 100 || time_since_last_fresh_build.as_millis() > 500;

    if should_queue_fresh {
        *last_queued_fresh_chunks = current;
    }

    let result = changes.iter_chunks(|cref| {
        let flags = cref.flags().unwrap();
        if flags.contains(ChunkFlags::FRESH) && !should_queue_fresh {
            return;
        }

        queued.insert(cref.pos());

        if flags.contains(ChunkFlags::EDGE_UPDATED) {
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        let offset = ivec3(x, y, z);
                        if offset == IVec3::ZERO {
                            continue;
                        }

                        let neighbor_pos = ChunkPos::from(IVec3::from(cref.pos()) + offset);

                        if !queued.contains(&neighbor_pos)
                            && !extra_queued.contains(&neighbor_pos)
                            && realm.chunk_manager.has_loaded_chunk(neighbor_pos)
                        {
                            if let Ok(cref) = realm.chunk_manager.get_loaded_chunk(neighbor_pos) {
                                if !cref.flags().unwrap().contains(ChunkFlags::GENERATING) {
                                    extra_queued.insert(neighbor_pos);
                                }
                            }
                        }
                    }
                }
            }
        }

        cref.update_flags(|flags| {
            flags.remove(ChunkFlags::UPDATED | ChunkFlags::FRESH | ChunkFlags::EDGE_UPDATED);
        })
        .unwrap();
    });

    if let Err(error) = result {
        error!("Error when iterating over changed chunks to mesh: {error}");
    }

    // let mut changed = hb::HashMap::new();

    // don't insert chunks that already exist in the ECS world (we'll get duplicates!!! :O)
    let mut inserted = hb::HashSet::new();
    for chunk in &chunks {
        inserted.insert(*chunk);
    }

    for &chunk_pos in queued.iter() {
        if !inserted.contains(&chunk_pos) {
            cmds.spawn((
                chunk_pos,
                ChunkEntity,
                ShouldExtract::default(),
                VisibilityBundle::default(),
                TransformBundle::from_transform(Transform::from_translation(
                    chunk_pos.worldspace_min().as_vec3(),
                )),
                Chunk::BOUNDING_BOX.to_aabb(),
            ));
        }

        workers.queue_job(chunk_pos);
        changes.acknowledge_change(chunk_pos);
    }

    for &chunk_pos in extra_queued.iter() {
        workers.queue_job(chunk_pos);
    }
}

pub fn insert_chunk_meshes(
    chunks: Query<(Entity, &ChunkPos, Option<&Handle<Mesh>>), With<ChunkEntity>>,
    mut cmds: Commands,
    workers: Res<MeshWorkerPool>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, &chunk_pos, mesh_handle) in &chunks {
        if let Some(mesher_output) = workers.get_new_mesh(chunk_pos) {
            let mut entity_cmds = cmds.entity(entity);

            match mesh_handle {
                Some(handle) => {
                    let Some(mesh) = meshes.get_mut(handle) else {
                        continue;
                    };

                    *mesh = mesher_output.mesh
                }
                None => {
                    let mesh_handle = meshes.add(mesher_output.mesh);
                    entity_cmds.insert(mesh_handle);
                }
            }

            cmds.entity(entity)
                .insert((mesher_output.quads, mesher_output.occlusion));
        }
    }
}
