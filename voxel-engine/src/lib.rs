extern crate crossbeam as cb;
extern crate derive_more as dm;
extern crate hashbrown as hb;
extern crate static_assertions as sa;
extern crate thiserror as te;

#[macro_use]
extern crate num_derive;

use bevy::{pbr::ExtendedMaterial, prelude::*};
use data::registries::Registries;
use render::meshing_algos::SimplePbrMesher;
use topo::{
    generator::{GenerateChunk, Generator, GeneratorChoice},
    realm::VoxelRealm,
};

pub mod data;
pub mod render;
pub mod topo;
pub mod util;

use crate::{
    data::textures::{check_textures, load_textures},
    render::{
        greedy_mesh_material::GreedyMeshMaterial,
        meshing_algos::GreedyMesher,
        systems::{build_meshes, insert_meshes, setup_mesh_builder},
    },
    topo::systems::generate_chunks_from_events,
};

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

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash, States)]
pub enum AppState {
    #[default]
    Setup,
    Finished,
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
        app.add_event::<GenerateChunk>();
        app.add_state::<AppState>();

        // app.add_systems(Startup, setup);
        app.add_systems(OnEnter(AppState::Setup), load_textures);
        app.add_systems(Update, check_textures.run_if(in_state(AppState::Setup)));
        app.add_systems(
            OnEnter(AppState::Finished),
            (
                setup_mesh_builder::<Hqm, Lqm>,
                apply_deferred,
                setup,
                generate_debug_chunks,
            )
                .chain(),
        );

        app.add_systems(
            PreUpdate,
            insert_meshes::<Hqm, Lqm>.run_if(in_state(AppState::Finished)),
        );
        app.add_systems(
            PostUpdate,
            (generate_chunks_from_events, build_meshes::<Hqm, Lqm>)
                .chain()
                .run_if(in_state(AppState::Finished)),
        );

        // app.add_systems(PreUpdate, insert_meshes::<Hqm, Lqm>);
        // app.add_systems(
        //     PostUpdate,
        //     (generate_chunks_from_events, build_meshes::<Hqm, Lqm>).chain(),
        // );
    }
}

fn generate_debug_chunks(mut events: EventWriter<GenerateChunk>) {
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                events.send(GenerateChunk {
                    pos: IVec3::new(x, y, z).into(),
                    generator: GeneratorChoice::Default,
                });
            }
        }
    }
}

fn setup(mut cmds: Commands, registries: Res<Registries>) {
    println!("running setup");
    let available_parallelism = std::thread::available_parallelism().unwrap();
    // let mut texture_reg_builder = VoxelTextureRegistryBuilder::new(server.as_ref());

    // texture_reg_builder
    //     .add_texture("textures/debug_texture.png")
    //     .unwrap();

    // let texture_registry = texture_reg_builder.finish(textures.as_mut()).unwrap();
    // let atlas_texture = texture_registry.atlas_texture();

    // let mut voxel_reg_builder = VoxelRegistryBuilder::new(&texture_registry);
    // voxel_reg_builder.register::<defaults::Void>();
    // voxel_reg_builder.register::<defaults::DebugVoxel>();

    // let voxel_registry = voxel_reg_builder.finish();
    // let registries = Registries::new(texture_registry, voxel_registry);

    // cmds.insert_resource(registries.clone());
    cmds.insert_resource(VoxelRealm::new());
    cmds.insert_resource(EngineThreadPool::new(available_parallelism.into()));
    cmds.insert_resource(DefaultGenerator(Generator::new(
        112456754,
        registries.clone(),
    )));

    // let mesh_builder = ParallelMeshBuilder::new(
    //     GreedyMesher::new(atlas_texture),
    //     SimplePbrMesher::new(),
    //     registries,
    // );

    // let hq = hqs.add(mesh_builder.hq_material());
    // cmds.insert_resource(HqMaterial(hq));

    // let lq = lqs.add(mesh_builder.lq_material());
    // cmds.insert_resource(LqMaterial(lq));

    // cmds.insert_resource(mesh_builder);
}
