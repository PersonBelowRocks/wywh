extern crate derive_more as dm;
extern crate derive_new as dn;
extern crate hashbrown as hb;
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
use diagnostics::VoxelEngineDiagnosticsPlugin;
use mip_texture_array::MippedArrayTexturePlugin;

use render::core::RenderCoreDebug;
use topo::{
    block::FullBlock,
    controller::{WorldController, WorldControllerSettings, WorldControllerSystems},
    world::{chunk_manager::ecs::ChunkManagerRes, ChunkManager},
};

pub mod data;
pub mod diagnostics;
pub mod render;
pub mod topo;
pub mod util;

use crate::{
    data::systems::{build_registries, check_textures, load_textures, VariantFolders},
    render::{core::RenderCore, meshing::controller::MeshController},
};

#[derive(Default)]
pub struct VoxelPlugin {
    pub variant_folders: Arc<Vec<PathBuf>>,
    pub render_core_debug: Option<RenderCoreDebug>,
}

#[derive(SystemSet, Hash, Debug, PartialEq, Eq, Clone)]
pub struct VoxelSystemSet;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, Hash, States)]
pub enum EngineState {
    #[default]
    Setup,
    Finished,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, SystemSet)]
pub enum CoreEngineSetup {
    BuildRegistries,
    InitializeChunkManager,
    Initialize,
}

impl Plugin for VoxelPlugin {
    fn build(&self, app: &mut App) {
        info!("Initializing voxel engine");

        app.add_plugins(VoxelEngineDiagnosticsPlugin)
            .add_plugins(WorldController {
                settings: WorldControllerSettings {
                    chunk_loading_handler_backlog_threshold: 100,
                    chunk_loading_handler_timeout: Duration::from_micros(20),
                    chunk_loading_max_stalling: Duration::from_millis(200),
                },
            });
        app.add_plugins(MeshController);
        app.add_plugins(RenderCore {
            debug: self.render_core_debug.clone(),
        });
        app.add_plugins(MippedArrayTexturePlugin::default());

        app.init_state::<EngineState>();

        app.insert_resource(VariantFolders::new(self.variant_folders.clone()));

        app.configure_sets(
            OnEnter(EngineState::Finished),
            (
                CoreEngineSetup::BuildRegistries,
                CoreEngineSetup::InitializeChunkManager,
                CoreEngineSetup::Initialize,
            )
                .chain(),
        );

        app.add_systems(OnEnter(EngineState::Setup), load_textures);
        app.add_systems(Update, check_textures.run_if(in_state(EngineState::Setup)));
        app.add_systems(
            OnEnter(EngineState::Finished),
            build_registries.in_set(CoreEngineSetup::BuildRegistries),
        );
    }
}
