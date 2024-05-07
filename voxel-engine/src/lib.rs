extern crate crossbeam as cb;
extern crate derive_more as dm;
extern crate derive_new as dn;
extern crate hashbrown as hb;
extern crate static_assertions as sa;
extern crate thiserror as te;

#[macro_use]
extern crate num_derive;

use std::{path::PathBuf, sync::Arc};

use bevy::prelude::*;
use data::{
    registries::{block::BlockVariantRegistry, Registries, Registry},
    resourcepath::rpath,
};
use mip_texture_array::MippedArrayTexturePlugin;
use topo::{block::FullBlock, world::VoxelRealm};

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
pub enum AppState {
    #[default]
    Setup,
    Finished,
}

impl Plugin for VoxelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshController);
        app.add_plugins(RenderCore);
        app.add_plugins(MippedArrayTexturePlugin::default());

        app.add_event::<GenerateChunk>();
        app.init_state::<AppState>();

        app.insert_resource(VariantFolders::new(self.variant_folders.clone()));
        app.insert_resource(GeneratorSeed(140));

        app.add_systems(OnEnter(AppState::Setup), load_textures);
        app.add_systems(Update, check_textures.run_if(in_state(AppState::Setup)));
        app.add_systems(
            OnEnter(AppState::Finished),
            (
                build_registries,
                setup,
                setup_terrain_generator_workers,
                generate_debug_chunks,
            )
                .chain(),
        );

        app.add_systems(
            PostUpdate,
            generate_chunks_from_events
                .chain()
                .run_if(in_state(AppState::Finished)),
        );
    }
}

fn generate_debug_chunks(mut events: EventWriter<GenerateChunk>) {
    const DIMS: i32 = 4;

    for x in -DIMS..=DIMS {
        for y in -DIMS..=DIMS {
            for z in -DIMS..=DIMS {
                events.send(GenerateChunk {
                    pos: IVec3::new(x, y, z).into(),
                });
            }
        }
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

    cmds.insert_resource(VoxelRealm::new(void));
}
