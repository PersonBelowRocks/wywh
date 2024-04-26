use std::fmt::Debug;

use bevy::prelude::*;

use crate::topo::world::ChunkPos;

#[derive(Debug)]
pub struct ChunkMesh {
    pub(crate) pos: ChunkPos,
    pub(crate) mesh: Mesh,
}

impl From<ChunkMesh> for Mesh {
    fn from(value: ChunkMesh) -> Self {
        value.mesh
    }
}

impl ChunkMesh {
    pub fn pos(&self) -> ChunkPos {
        self.pos
    }
}
