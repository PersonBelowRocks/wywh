mod ecs;
mod workers;

use bevy::prelude::*;

use crate::render::quad::{ChunkQuads, GpuQuad};

use self::ecs::{ChunkMeshStorage, RemeshChunk};

#[derive(Clone)]
pub struct ChunkMeshData {
    pub index_buffer: Vec<u32>,
    pub quads: ChunkQuads,
}

pub struct MeshController;

impl Plugin for MeshController {
    fn build(&self, app: &mut App) {
        app.init_resource::<ChunkMeshStorage>()
            .add_event::<RemeshChunk>();
    }
}
