use bevy::{pbr::ExtendedMaterial, prelude::*};

use crate::{
    data::registry::{Registries, VoxelTextureAtlas},
    HqMaterial, LqMaterial,
};

use super::{
    greedy_mesh_material::GreedyMeshMaterial,
    mesh_builder::{Mesher, ParallelMeshBuilder},
    meshing_algos::{GreedyMesher, SimplePbrMesher},
};

pub(crate) fn setup_mesh_builder<Hqm: Mesher, Lqm: Mesher>(
    mut cmds: Commands,

    atlas_texture: Res<VoxelTextureAtlas>,
    registries: Res<Registries>,

    mut hqs: ResMut<Assets<ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>>>,
    mut lqs: ResMut<Assets<StandardMaterial>>,
) {
    let mesh_builder = ParallelMeshBuilder::new(
        GreedyMesher::new(atlas_texture.0.clone()),
        SimplePbrMesher::new(),
        registries.as_ref().clone(),
    );

    let hq = hqs.add(mesh_builder.hq_material());
    cmds.insert_resource(HqMaterial(hq));

    let lq = lqs.add(mesh_builder.lq_material());
    cmds.insert_resource(LqMaterial(lq));

    cmds.insert_resource(mesh_builder);
}
