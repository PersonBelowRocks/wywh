use bevy::{
    pbr::{wireframe::Wireframe, ExtendedMaterial},
    prelude::*,
    render::{
        render_resource::{BufferInitDescriptor, BufferUsages},
        renderer::RenderDevice,
        texture::ImageSampler,
    },
};

use crate::{
    data::{
        registries::Registries,
        systems::{VoxelColorTextureAtlas, VoxelNormalTextureAtlas},
    },
    render::{adjacency::AdjacentTransparency, core::mat::VxlChunkMaterial},
    topo::{chunk::Chunk, realm::VoxelRealm},
    ChunkEntity,
};

use super::{
    mesh_builder::{Mesher, ParallelMeshBuilder},
    meshing::greedy::algorithm::{GreedyMesher, SimplePbrMesher},
};

pub(crate) fn setup_mesh_builder<Hqm: Mesher, Lqm: Mesher>(
    mut cmds: Commands,
    registries: Res<Registries>,
) {
    let mesh_builder = ParallelMeshBuilder::new(
        GreedyMesher::new(registries.clone()),
        SimplePbrMesher::new(),
        registries.as_ref().clone(),
    );

    cmds.insert_resource(mesh_builder);
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
    mut cmds: Commands,
    mut mesh_builder: ResMut<ParallelMeshBuilder<HQM, LQM>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ExtendedMaterial<StandardMaterial, VxlChunkMaterial>>>,
    gpu: Res<RenderDevice>,
    texture_atlas: Res<VoxelColorTextureAtlas>,
    normal_atlas: Res<VoxelNormalTextureAtlas>,
) {
    use slice_of_array::prelude::*;

    for finished_mesh in mesh_builder.finished_meshes() {
        debug!("Inserting mesh at {:?}", finished_mesh.pos);
        let pos = (*finished_mesh.pos * Chunk::SIZE).as_vec3() + Vec3::splat(0.5);

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

pub(crate) fn configure_sampling(
    color_atlas_handle: Res<VoxelColorTextureAtlas>,
    normal_atlas_handle: Res<VoxelNormalTextureAtlas>,
    mut images: ResMut<Assets<Image>>,
) {
    let texture = images.get_mut(&color_atlas_handle.0).unwrap();
    texture.sampler = ImageSampler::nearest();

    let texture = images.get_mut(&normal_atlas_handle.0).unwrap();
    texture.sampler = ImageSampler::nearest();
}
