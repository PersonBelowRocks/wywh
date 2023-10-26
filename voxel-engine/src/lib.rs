extern crate crossbeam as cb;
extern crate derive_more as dm;
extern crate hashbrown as hb;
extern crate thiserror as te;

#[macro_use]
extern crate num_derive;

use std::{borrow::BorrowMut, sync::mpsc};

use bevy::{
    ecs::schedule::{ScheduleBuildSettings, ScheduleLabel},
    pbr::{wireframe::Wireframe, ExtendedMaterial},
    prelude::*,
    render::view::NoFrustumCulling,
};
use data::{
    registry::{Registries, VoxelRegistryBuilder, VoxelTextureRegistryBuilder},
    tile::VoxelId,
};
use render::{
    adjacency::AdjacentTransparency,
    mesh::ChunkMesh,
    mesh_builder::{Mesher, ParallelMeshBuilder},
    meshing_algos::SimplePbrMesher,
};
use topo::{
    access::WriteAccess,
    chunk::Chunk,
    containers::{ChunkVoxelDataStorage, RawChunkVoxelContainer},
    generator::{GenerateChunk, Generator},
    realm::VoxelRealm,
};

pub mod data;
pub mod defaults;
pub mod render;
pub mod topo;
pub mod util;

pub use render::material::VoxelChunkMaterial;

use crate::render::{greedy_mesh_material::GreedyMeshMaterial, meshing_algos::GreedyMesher};

pub struct VoxelPlugin;

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct VoxelSystemSet;

#[derive(Resource, Deref)]
pub struct EngineThreadPool(rayon::ThreadPool);

#[derive(Resource, Deref)]
pub struct HqMaterial<M: Material>(Handle<M>);

#[derive(Resource, Deref)]
pub struct LqMaterial<M: Material>(Handle<M>);

#[derive(Resource, Deref)]
pub struct DefaultGenerator(Generator);

#[derive(Component)]
pub struct ChunkEntity;

impl EngineThreadPool {
    pub fn new(num_threads: usize) -> Self {
        let internal_pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        Self(internal_pool)
    }
}

impl Plugin for VoxelPlugin {
    fn build(&self, app: &mut App) {
        type Hqm = GreedyMesher;
        type Lqm = SimplePbrMesher;

        // app.add_plugins(MaterialPlugin::<VoxelChunkMaterial>::default());
        // app.add_plugins(MaterialPlugin::<GreedyMeshMaterial>::default());
        app.add_plugins(MaterialPlugin::<
            ExtendedMaterial<StandardMaterial, GreedyMeshMaterial>,
        >::default());
        app.add_event::<GenerateChunk<VoxelId>>();

        app.add_systems(
            Startup,
            (
                setup::<Hqm, Lqm>,
                insert_hq_material::<Hqm, Lqm>,
                insert_lq_material::<Hqm, Lqm>,
            ),
        );

        app.add_systems(PreUpdate, insert_meshes::<Hqm, Lqm>);
        app.add_systems(
            PostUpdate,
            (generate_chunks_from_events, build_meshes::<Hqm, Lqm>).chain(),
        );
    }
}

fn generate_chunks_from_events(
    mut reader: EventReader<GenerateChunk<VoxelId>>,
    realm: Res<VoxelRealm>,
    generator: Res<DefaultGenerator>,
) {
    for event in reader.read() {
        let mut container = RawChunkVoxelContainer::<VoxelId>::Empty;
        let mut access = container.auto_access(event.default_value);

        generator.write_to_chunk(event.pos, &mut access).unwrap();

        let chunk = Chunk::new_from_container(container);
        realm.chunk_manager.set_loaded_chunk(event.pos, chunk)
    }
}

fn build_meshes<HQM: Mesher, LQM: Mesher>(
    realm: Res<VoxelRealm>,
    mut mesh_builder: NonSendMut<ParallelMeshBuilder<HQM, LQM>>,
) {
    for chunk in realm.chunk_manager.changed_chunks() {
        // TODO: adjacency system feels a little half baked... maybe do some caching of some sort?
        let adjacency = AdjacentTransparency::new(chunk.pos(), &realm.chunk_manager);
        let id = mesh_builder.queue_chunk(chunk, adjacency);

        println!("Chunk meshing task with ID {:?} started", id);
    }
}

fn insert_meshes<HQM: Mesher, LQM: Mesher>(
    mut cmds: Commands,
    mut mesh_builder: NonSendMut<ParallelMeshBuilder<HQM, LQM>>,
    mut meshes: ResMut<Assets<Mesh>>,
    hq_material: Res<HqMaterial<HQM::Material>>,
) {
    for finished_mesh in mesh_builder.finished_meshes().into_iter() {
        println!("Inserting mesh at {:?}", finished_mesh.pos());
        let pos = (*finished_mesh.pos() * Chunk::SIZE).as_vec3() + Vec3::splat(0.5);

        cmds.spawn(MaterialMeshBundle {
            mesh: meshes.add(finished_mesh.into()),
            material: hq_material.clone(),
            transform: Transform::from_translation(pos),

            ..default()
        })
        // .insert(Chunk::BOUNDING_BOX.to_aabb())
        .insert((ChunkEntity, Chunk::BOUNDING_BOX.to_aabb(), Wireframe));
    }
}

fn insert_hq_material<HQM: Mesher, LQM: Mesher>(
    mut cmds: Commands,
    mut materials: ResMut<Assets<HQM::Material>>,
    mesh_builder: NonSend<ParallelMeshBuilder<HQM, LQM>>,
) {
    let hq = materials.add(mesh_builder.hq_material());
    cmds.insert_resource(HqMaterial(hq))
}

fn insert_lq_material<HQM: Mesher, LQM: Mesher>(
    mut cmds: Commands,
    mut materials: ResMut<Assets<LQM::Material>>,
    mesh_builder: NonSend<ParallelMeshBuilder<HQM, LQM>>,
) {
    let lq = materials.add(mesh_builder.lq_material());
    cmds.insert_resource(LqMaterial(lq))
}

fn setup<HQM: Mesher, LQM: Mesher>(
    mut cmds: Commands,
    server: Res<AssetServer>,
    mut textures: ResMut<Assets<Image>>,
) {
    let available_parallelism = std::thread::available_parallelism().unwrap();
    let mut texture_reg_builder =
        VoxelTextureRegistryBuilder::new(server.as_ref(), textures.as_mut());

    texture_reg_builder
        .add_texture("textures/debug_texture.png")
        .unwrap();

    let texture_registry = texture_reg_builder.finish();
    let atlas_texture = texture_registry.atlas_texture();

    let mut voxel_reg_builder = VoxelRegistryBuilder::new(&texture_registry);
    voxel_reg_builder.register::<defaults::Void>();
    voxel_reg_builder.register::<defaults::DebugVoxel>();

    let voxel_registry = voxel_reg_builder.finish();
    let registries = Registries::new(texture_registry, voxel_registry);

    cmds.insert_resource(registries.clone());
    cmds.insert_resource(VoxelRealm::new());
    cmds.insert_resource(EngineThreadPool::new(available_parallelism.into()));
    cmds.insert_resource(DefaultGenerator(Generator::new(112456754)));

    cmds.insert_resource(ParallelMeshBuilder::new(
        GreedyMesher::new(atlas_texture),
        SimplePbrMesher::new(),
        registries,
    ));
}
