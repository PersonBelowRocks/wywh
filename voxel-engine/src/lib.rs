extern crate hashbrown as hb;
extern crate thiserror as te;

use bevy::prelude::*;
use chunk::{Chunk, ChunkVoxelData};
use render::{mesh::ChunkMesh};
use tile::VoxelId;

mod chunk;
mod error;
mod registry;
mod tile;
mod util;
mod world;
mod render;

pub use render::material::VoxelChunkMaterial;

pub struct VoxelPlugin;

impl Plugin for VoxelPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<VoxelChunkMaterial>::default());
        app.add_systems(Startup, setup);
    }
}

fn setup(
    mut cmds: Commands, 
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<VoxelChunkMaterial>>,
) {

    let mut chunk = Chunk::new(ChunkVoxelData::new(0.into()));
    chunk.set_voxel([5, 5, 5].into(), 1.into());
    chunk.set_voxel([5, 6, 5].into(), 1.into());
    chunk.set_voxel([5, 7, 5].into(), 1.into());

    chunk.set_voxel([0, 0, 0].into(), 1.into());

    let chunk_mesh = ChunkMesh::build(&chunk).unwrap();

    let mesh = meshes.add(chunk_mesh.into());
    let material = materials.add(VoxelChunkMaterial {});

    cmds.spawn(MaterialMeshBundle {
        mesh,
        material,
        .. default()
    });

}
