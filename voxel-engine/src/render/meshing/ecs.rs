use std::{convert::identity, sync::Arc};

use bevy::{ecs::system::lifetimeless::SRes, prelude::*, tasks::AsyncComputeTaskPool};
use dashmap::DashMap;

use crate::util::result::ResultFlattening;
use crate::{
    data::registries::Registries,
    render::{
        mesh_builder::{Context, Mesher, MesherOutput},
        meshing::error::ChunkMeshingError,
        quad::ChunkQuads,
    },
    topo::{chunk::ChunkPos, realm::VoxelRealm},
    ChunkEntity,
};

use super::MeshWorkerPool;

pub fn setup_chunk_meshing_workers<M: Mesher + Resource>(
    mut cmds: Commands,
    registries: Res<Registries>,
    realm: Res<VoxelRealm>,
    mesher: Res<M>,
) {
    let pool = AsyncComputeTaskPool::get();

    let worker_count = pool.thread_num();

    let worker_pool = MeshWorkerPool::<M>::new(
        worker_count,
        pool,
        mesher.clone(),
        registries.clone(),
        realm.chunk_manager.clone(),
    );

    cmds.insert_resource(worker_pool);
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
            cmds.spawn((chunk_pos, ChunkEntity));
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

            info!("Inserted mesh for chunk '{chunk_pos}'")
        }
    }
}
