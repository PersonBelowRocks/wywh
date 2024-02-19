use bevy::{
    pbr::ExtendedMaterial,
    prelude::*,
    render::{renderer::RenderDevice, texture::ImageSampler},
};

use crate::{
    data::systems::{VoxelColorArrayTexture, VoxelNormalArrayTexture},
    render::{adjacency::AdjacentTransparency, core::mat::VxlChunkMaterial},
    topo::{chunk::Chunk, realm::VoxelRealm},
};

use super::{
    mesh_builder::{Mesher, ParallelMeshBuilder},
    meshing::greedy::algorithm::{GreedyMesher, SimplePbrMesher},
};

pub(crate) fn setup_meshers(mut cmds: Commands) {
    cmds.insert_resource(GreedyMesher::new());
    cmds.insert_resource(SimplePbrMesher::new());
}

pub(crate) fn build_meshes<HQM: Mesher, LQM: Mesher>(
    realm: Res<VoxelRealm>,
    mut mesh_builder: ResMut<ParallelMeshBuilder<HQM, LQM>>,
) {
    for chunk in realm.chunk_manager.changed_chunks() {
        // TODO: adjacency system feels a little half baked... maybe do some caching of some sort?
        let adjacency = AdjacentTransparency::new(chunk.pos(), &realm.chunk_manager);
        let id = mesh_builder.queue_chunk(chunk, adjacency);

        debug!("Chunk meshing task with ID {:?} started", id);
    }
}

pub(crate) fn insert_meshes<HQM: Mesher, LQM: Mesher>(
    _cmds: Commands,
    mut mesh_builder: ResMut<ParallelMeshBuilder<HQM, LQM>>,
    _meshes: ResMut<Assets<Mesh>>,
    _materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, VxlChunkMaterial>>>,
    _gpu: Res<RenderDevice>,
    _texture_atlas: Res<VoxelColorArrayTexture>,
    _normal_atlas: Res<VoxelNormalArrayTexture>,
) {
    for finished_mesh in mesh_builder.finished_meshes() {
        debug!("Inserting mesh at {:?}", finished_mesh.pos);
        let _pos = (*finished_mesh.pos * Chunk::SIZE).as_vec3() + Vec3::splat(0.5);

        // TODO: insert chunk meshes so that the render core can extract it
        todo!()
        // let material = {
        //     let occlusion_map = gpu.create_buffer_with_data(&BufferInitDescriptor {
        //         label: Some("occlusion_buffer"),
        //         contents: finished_mesh.output.occlusion.as_buffer().flat(),
        //         usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
        //     });

        //     VxlChunkMaterial {
        //         faces: faces.0.as_ref().unwrap().clone(),
        //         occlusion: occlusion_map,
        //     }
        // };

        // cmds.spawn(MaterialMeshBundle {
        //     mesh: meshes.add(finished_mesh.output.mesh),
        //     material: materials.add(ExtendedMaterial {
        //         base: StandardMaterial {
        //             base_color_texture: Some(texture_atlas.0.clone()),
        //             normal_map_texture: Some(normal_atlas.0.clone()),
        //             perceptual_roughness: 1.0,
        //             reflectance: 0.0,
        //             // base_color: Color::rgb(0.5, 0.5, 0.65),
        //             ..default()
        //         },
        //         extension: material,
        //     }),
        //     transform: Transform::from_translation(pos),

        //     ..default()
        // })
        // .insert((Wireframe, ChunkEntity, Chunk::BOUNDING_BOX.to_aabb()));
    }
}
