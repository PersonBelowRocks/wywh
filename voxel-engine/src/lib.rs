extern crate crossbeam as cb;
extern crate derive_more as dm;
extern crate hashbrown as hb;
extern crate static_assertions as sa;
extern crate thiserror as te;

#[macro_use]
extern crate num_derive;

use std::{path::PathBuf, sync::Arc};

use bevy::prelude::*;
use data::{
    registries::{variant::VariantRegistry, Registries, Registry},
    resourcepath::rpath,
};
use mip_texture_array::MippedArrayTexturePlugin;
use render::meshing::greedy::algorithm::SimplePbrMesher;
use topo::{
    chunk_ref::ChunkVoxelOutput,
    generator::{GenerateChunk, Generator, GeneratorChoice},
    realm::VoxelRealm,
};

pub mod data;
pub mod render;
pub mod topo;
pub mod util;

use crate::{
    data::{
        systems::{build_registries, check_textures, load_textures, VariantFolders},
        tile::Transparency,
    },
    render::{
        core::RenderCore,
        meshing::{
            ecs::{insert_chunk_meshes, queue_chunk_meshing_tasks, setup_chunk_meshing_workers},
            greedy::algorithm::GreedyMesher,
        },
        systems::setup_meshers,
    },
    topo::systems::generate_chunks_from_events,
};

pub struct VoxelPlugin {
    variant_folders: Arc<Vec<PathBuf>>,
}

impl VoxelPlugin {
    pub fn new(variant_folders: Vec<PathBuf>) -> Self {
        VoxelPlugin {
            variant_folders: Arc::new(variant_folders),
        }
    }
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct VoxelSystemSet;

#[derive(Resource, Deref)]
pub struct DefaultGenerator(Generator);

#[derive(Component)]
pub struct ChunkEntity;

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
        app.add_plugins(RenderCore);
        app.add_plugins(MippedArrayTexturePlugin::default());

        app.add_event::<GenerateChunk>();
        app.add_state::<AppState>();

        app.insert_resource(VariantFolders::new(self.variant_folders.clone()));

        // app.add_systems(Startup, setup);
        app.add_systems(OnEnter(AppState::Setup), load_textures);
        app.add_systems(Update, check_textures.run_if(in_state(AppState::Setup)));
        app.add_systems(
            OnEnter(AppState::Finished),
            (
                build_registries,
                apply_deferred,
                setup_meshers,
                apply_deferred,
                setup,
                setup_chunk_meshing_workers::<Hqm>,
                generate_debug_chunks,
            )
                .chain(),
        );

        app.add_systems(
            PreUpdate,
            insert_chunk_meshes::<Hqm>.run_if(in_state(AppState::Finished)),
        );
        app.add_systems(
            PostUpdate,
            (
                generate_chunks_from_events,
                queue_chunk_meshing_tasks::<Hqm>,
            )
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
    const DIMS: i32 = 4;

    for x in -DIMS..=DIMS {
        for y in -DIMS..=DIMS {
            for z in -DIMS..=DIMS {
                events.send(GenerateChunk {
                    pos: IVec3::new(x, y, z).into(),
                    generator: GeneratorChoice::Default,
                });
            }
        }
    }
}

fn setup(mut cmds: Commands, registries: Res<Registries>) {
    let varreg = registries.get_registry::<VariantRegistry>().unwrap();
    let void_cvo = ChunkVoxelOutput {
        variant: varreg.get_id(&rpath("void")).unwrap(),
        transparency: Transparency::Transparent,
        rotation: None,
    };

    cmds.insert_resource(VoxelRealm::new(void_cvo));
    cmds.insert_resource(DefaultGenerator(Generator::new(
        112456754,
        registries.as_ref(),
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
