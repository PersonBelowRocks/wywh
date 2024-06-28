extern crate binary_heap_plus as bhp;
extern crate crossbeam as cb;
extern crate derive_more as dm;
extern crate derive_new as dn;
extern crate hashbrown as hb;
extern crate overload;
extern crate static_assertions as sa;
extern crate thiserror as te;

#[macro_use]
extern crate num_derive;

use std::{path::PathBuf, sync::Arc, time::Duration};

use bevy::prelude::*;
use data::{
    registries::{block::BlockVariantRegistry, Registries, Registry},
    resourcepath::rpath,
};
use mip_texture_array::MippedArrayTexturePlugin;

use topo::{
    block::FullBlock,
    controller::{
        ChunkEcsPermits, WorldController, WorldControllerSettings, WorldControllerSystems,
    },
    world::{realm::ChunkManagerResource, ChunkManager},
};

pub mod data;
pub mod render;
pub mod topo;
pub mod util;

#[cfg(test)]
pub mod testing_utils;

use crate::{
    data::systems::{build_registries, check_textures, load_textures, VariantFolders},
    render::{core::RenderCore, meshing::controller::MeshController},
    topo::worldgen::{
        ecs::{generate_chunks_from_events, setup_terrain_generator_workers, GeneratorSeed},
        generator::GenerateChunk,
    },
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

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash, States)]
pub enum EngineState {
    #[default]
    Setup,
    Finished,
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Hash, Debug, SystemSet)]
pub struct CoreEngineSetup;

impl Plugin for VoxelPlugin {
    fn build(&self, app: &mut App) {
        info!("Building voxel plugin");

        app.add_plugins(WorldController {
            settings: WorldControllerSettings {
                chunk_loading_handler_backlog_threshold: 100,
                chunk_loading_handler_timeout: Duration::from_micros(20),
                chunk_loading_max_stalling: Duration::from_millis(200),
            },
        });
        app.add_plugins(MeshController);
        app.add_plugins(RenderCore);
        app.add_plugins(MippedArrayTexturePlugin::default());

        app.add_event::<GenerateChunk>();
        app.init_state::<EngineState>();

        app.insert_resource(VariantFolders::new(self.variant_folders.clone()));
        app.insert_resource(GeneratorSeed(140));

        app.add_systems(OnEnter(EngineState::Setup), load_textures);
        app.add_systems(Update, check_textures.run_if(in_state(EngineState::Setup)));
        app.add_systems(
            OnEnter(EngineState::Finished),
            (build_registries, setup, setup_terrain_generator_workers)
                .chain()
                .in_set(CoreEngineSetup),
        );

        app.add_systems(
            FixedPostUpdate,
            generate_chunks_from_events
                .run_if(in_state(EngineState::Finished))
                .after(WorldControllerSystems::CoreEvents),
        );
    }
}

fn setup(mut cmds: Commands, registries: Res<Registries>) {
    let varreg = registries.get_registry::<BlockVariantRegistry>().unwrap();
    let void = FullBlock {
        rotation: None,
        id: varreg
            .get_id(&rpath(BlockVariantRegistry::RPATH_VOID))
            .unwrap(),
    };

    let chunk_manager = ChunkManager::new(void);

    cmds.init_resource::<ChunkEcsPermits>();
    cmds.insert_resource(ChunkManagerResource(Arc::new(chunk_manager)));
}
