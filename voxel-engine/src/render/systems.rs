use bevy::{
    pbr::{wireframe::Wireframe, ExtendedMaterial},
    prelude::*,
    render::texture::ImageSampler,
};

use crate::{
    data::{
        registries::Registries,
        systems::{VoxelColorTextureAtlas, VoxelNormalTextureAtlas},
    },
    render::adjacency::AdjacentTransparency,
    topo::{chunk::Chunk, realm::VoxelRealm},
    ChunkEntity, HqMaterial, LqMaterial,
};

use super::{
    mesh_builder::{Mesher, ParallelMeshBuilder},
    meshing::greedy::{
        algorithm::{GreedyMesher, SimplePbrMesher},
        material::GreedyMeshMaterial,
    },
};

pub(crate) fn setup_mesh_builder<Hqm: Mesher, Lqm: Mesher>(
    mut cmds: Commands,

    _atlas_texture: Res<VoxelColorTextureAtlas>,
    registries: Res<Registries>,

    mut hqs: ResMut<Assets<ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>>>,
    mut lqs: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_builder = ParallelMeshBuilder::new(
        GreedyMesher::new(registries.clone()),
        SimplePbrMesher::new(),
        registries.as_ref().clone(),
    );

    let hq = hqs.add(mesh_builder.hq_material());
    cmds.insert_resource(HqMaterial(hq));

    let lq = lqs.add(mesh_builder.lq_material());
    cmds.insert_resource(LqMaterial(lq));

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
    hq_material: Res<HqMaterial<HQM::Material>>,
) {
    for finished_mesh in mesh_builder.finished_meshes().into_iter() {
        debug!("Inserting mesh at {:?}", finished_mesh.pos());
        let pos = (*finished_mesh.pos() * Chunk::SIZE).as_vec3() + Vec3::splat(0.5);

        cmds.spawn(MaterialMeshBundle {
            mesh: meshes.add(finished_mesh.into()),
            material: hq_material.clone(),
            transform: Transform::from_translation(pos),

            ..default()
        })
        .insert((Wireframe, ChunkEntity, Chunk::BOUNDING_BOX.to_aabb()));
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
