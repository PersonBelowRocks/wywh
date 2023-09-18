extern crate derive_more as dm;
extern crate hashbrown as hb;
extern crate thiserror as te;

#[macro_use]
extern crate num_derive;

use std::sync::mpsc;

use bevy::{
    ecs::schedule::{ScheduleBuildSettings, ScheduleLabel},
    prelude::*,
    render::view::NoFrustumCulling,
};
use data::tile::VoxelId;
use render::{adjacency::AdjacentTransparency, mesh::ChunkMesh};
use topo::{
    access::WriteAccess,
    chunk::Chunk,
    containers::{ChunkVoxelDataStorage, RawChunkVoxelContainer},
    generator::{GenerateChunk, Generator},
    realm::VoxelRealm,
};

pub mod data;
pub mod render;
pub mod topo;
pub mod util;

pub use render::material::VoxelChunkMaterial;

pub struct VoxelPlugin;

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct VoxelSystemSet;

#[derive(Resource, Deref)]
pub struct EngineThreadPool(rayon::ThreadPool);

#[derive(Resource, Deref)]
pub struct DefaultVoxelChunkMaterial(Handle<VoxelChunkMaterial>);

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
        app.add_plugins(MaterialPlugin::<VoxelChunkMaterial>::default());
        app.add_event::<GenerateChunk<VoxelId>>();
        app.add_systems(Startup, setup);
        app.add_systems(
            PostUpdate,
            (generate_chunks_from_events, build_meshes).chain(),
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

fn build_meshes(
    pool: Res<EngineThreadPool>,
    realm: Res<VoxelRealm>,
    mut cmds: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    chunk_material: Res<DefaultVoxelChunkMaterial>,
) {
    let (finished_tx, finished_rx) = mpsc::channel::<ChunkMesh>();

    pool.scope(move |scope| {
        let realm_ref = realm.as_ref();
        for chunk in realm.chunk_manager.changed_chunks() {
            let tx = finished_tx.clone();
            let adjacency = AdjacentTransparency::new(chunk.pos, &realm_ref.chunk_manager);
            scope.spawn(move |_| {
                println!("Building chunk mesh for {0}", chunk.pos);

                let chunk_mesh = ChunkMesh::build(&chunk, &adjacency);

                println!("Finished building chunk mesh for {0}", chunk.pos);
                tx.send(chunk_mesh).unwrap();
            })
        }
    });

    for chunk_mesh in finished_rx.into_iter() {
        let pos = (*chunk_mesh.pos() * Chunk::SIZE).as_vec3();

        println!("Spawning MaterialMesh entity for chunk with chunk position {0}, and world position {1}", chunk_mesh.pos(), pos);
        cmds.spawn(MaterialMeshBundle {
            mesh: meshes.add(chunk_mesh.into()),
            material: chunk_material.clone(),
            transform: Transform::from_translation(pos),

            ..default()
        })
        .insert(NoFrustumCulling)
        .insert(ChunkEntity);
    }
}

fn setup(mut cmds: Commands, mut materials: ResMut<Assets<VoxelChunkMaterial>>) {
    let available_parallelism = std::thread::available_parallelism().unwrap();

    let handle = materials.add(VoxelChunkMaterial {});

    cmds.insert_resource(DefaultVoxelChunkMaterial(handle));
    cmds.insert_resource(VoxelRealm::new());
    cmds.insert_resource(EngineThreadPool::new(available_parallelism.into()));
    cmds.insert_resource(DefaultGenerator(Generator::new(1337)));
}

/*
fn setup(
    mut cmds: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoxelChunkMaterial>>,
) {
    let mut chunk = Chunk::new(ChunkVoxelDataStorage::new(0.into()));

    #[allow(unused)]
    {
        let mut access = chunk.voxels.access();
        access.set([5, 5, 5].into(), 1.into());
        access.set([5, 6, 5].into(), 1.into());
        access.set([5, 7, 5].into(), 1.into());

        access.set([0, 0, 0].into(), 1.into());
        access.set([0, 1, 0].into(), 1.into());

        access.set([0, 1, 2].into(), 1.into());
    }

    let chunk_mesh = ChunkMesh::build(&chunk).unwrap();

    let mesh = meshes.add(chunk_mesh.into());
    let material = materials.add(VoxelChunkMaterial {});

    cmds.spawn(MaterialMeshBundle {
        mesh,
        material,
        ..default()
    })
    // TODO: culling system
    .insert(NoFrustumCulling);
}
*/
