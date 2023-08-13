use bevy::prelude::*;

use crate::chunk::Chunk;

pub struct World {
    chunks: hb::HashMap<IVec3, Chunk>,
    updated_last_tick: hb::HashSet<IVec3>,
}

impl World {
    pub fn new() -> Self {
        Self {
            chunks: default(),
            updated_last_tick: default(),
        }
    }

    pub fn insert_chunk(&mut self, pos: IVec3, chunk: Chunk) {
        self.chunks.insert(pos, chunk);
    }

    pub fn get_chunk(&self, pos: IVec3) -> Option<&Chunk> {
        self.chunks.get(&pos)
    }
}
