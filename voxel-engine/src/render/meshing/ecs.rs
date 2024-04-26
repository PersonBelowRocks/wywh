use std::sync::atomic::{AtomicBool, Ordering};

use bevy::tasks::{TaskPool, TaskPoolBuilder};
use bevy::{prelude::*, tasks::AsyncComputeTaskPool};

use crate::topo::chunk::Chunk;

use crate::{
    data::registries::Registries,
    topo::{chunk::ChunkPos, realm::VoxelRealm},
    ChunkEntity,
};

use super::greedy::algorithm::{GreedyMesher, SimplePbrMesher};
use super::{MeshWorkerPool, Mesher};

// TODO: the whole "meshers as resources" thing does not makes sense
// anymore, so get rid of the last remains of it to clean up the codebase
pub(crate) fn setup_meshers(mut cmds: Commands) {
    cmds.insert_resource(GreedyMesher::new());
    cmds.insert_resource(SimplePbrMesher::new());
}

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

pub fn setup_chunk_meshing_workers<M: Mesher + Resource>(
    mut cmds: Commands,
    registries: Res<Registries>,
    realm: Res<VoxelRealm>,
    mesher: Res<M>,
) {
    let task_pool = TaskPoolBuilder::new()
        .thread_name("Mesh Worker Task Pool".into())
        .build();

    let worker_pool = MeshWorkerPool::<M>::new(
        task_pool.thread_num(),
        &task_pool,
        mesher.clone(),
        registries.clone(),
        realm.chunk_manager.clone(),
    );

    cmds.insert_resource(worker_pool);
    cmds.insert_resource(MeshWorkerTaskPool(task_pool));
}

pub fn queue_chunk_meshing_tasks<M: Mesher>(
    mut cmds: Commands,
    chunks: Query<&ChunkPos, With<ChunkEntity>>,
    realm: Res<VoxelRealm>,
    workers: Res<MeshWorkerPool<M>>,
) {
    let mut changed = hb::HashMap::new();
    for chunk in realm.chunk_manager.changed_chunks() {
        // the boolean value in this tuple is for whether or not we should insert this chunk
        // into the ECS world
        workers.queue_job(chunk.pos());
        changed.insert(chunk.pos(), (chunk, true));
    }

    // don't insert chunks that already exist in the ECS world (we'll get duplicates!!! :O)
    for chunk in &chunks {
        changed
            .get_mut(chunk)
            .map(|(_, should_insert)| *should_insert = false);
    }

    for (&chunk_pos, (_cref, should_insert)) in changed.iter() {
        if *should_insert {
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
    }
}

pub fn insert_chunk_meshes<M: Mesher>(
    chunks: Query<(Entity, &ChunkPos, Option<&Handle<Mesh>>), With<ChunkEntity>>,
    mut cmds: Commands,
    workers: Res<MeshWorkerPool<M>>,
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
