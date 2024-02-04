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

#[derive(Clone, Resource, Deref)]
pub struct FinishedChunks(Arc<DashMap<ChunkPos, MesherOutput>>);

pub fn queue_chunk_meshing_tasks<Hqm: Mesher + Resource, Lqm: Mesher + Resource>(
    finished_chunks: SRes<FinishedChunks>,
    realm: SRes<VoxelRealm>,
    registries: SRes<Registries>,
    hqm: SRes<Hqm>,
) {
    let pool = AsyncComputeTaskPool::get();

    let realm = realm.into_inner();
    let registries = registries.into_inner();
    let hqm = hqm.into_inner();

    for chunk in realm.chunk_manager.changed_chunks() {
        let chunks = finished_chunks.clone();

        pool.spawn(async move {
            let chunk_pos = chunk.pos();
            let result = realm
                .chunk_manager
                .with_neighbors(chunk_pos, |neighbors| {
                    let context = Context {
                        neighbors,
                        registries,
                    };

                    chunk
                        .with_read_access(|access| {
                            hqm.build(access, context).map_err(ChunkMeshingError::from)
                        })
                        .map_err(ChunkMeshingError::from)
                })
                .map_err(ChunkMeshingError::from)
                .custom_flatten()
                .custom_flatten();

            match result {
                Ok(output) => {
                    chunks.0.insert(chunk_pos, output);
                }

                Err(error) => error!("Error building chunk mesh: {error}"),
            }
        })
        .detach();
    }
}

pub fn insert_chunks(
    chunks: Query<(Entity, &ChunkPos, Option<&Handle<Mesh>>), With<ChunkEntity>>,
    mut cmds: Commands,
    mut finished_meshes: ResMut<FinishedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, chunk_pos, mesh_handle) in &chunks {
        if let Some((_, mesher_output)) = finished_meshes.remove(chunk_pos) {
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
