use bevy::{math::ivec2, prelude::IVec2};

use crate::{topo::chunk::Chunk, util::SquareArray};

#[derive(Clone)]
pub(crate) struct ChunkSliceMask(SquareArray<{ Chunk::SUBDIVIDED_CHUNK_USIZE }, bool>);

impl ChunkSliceMask {
    pub fn new() -> Self {
        Self([[false; Chunk::SUBDIVIDED_CHUNK_USIZE]; Chunk::SUBDIVIDED_CHUNK_USIZE])
    }

    pub fn contains(pos: IVec2) -> bool {
        pos.cmpge(ivec2(0, 0)).all() && pos.cmplt(ivec2(Chunk::SIZE, Chunk::SIZE)).all()
    }

    pub fn mask(&mut self, pos: IVec2) -> bool {
        if Self::contains(pos) {
            self.0[pos.x as usize][pos.y as usize] = true;

            true
        } else {
            false
        }
    }

    pub fn mask_region(&mut self, from: IVec2, to: IVec2) -> bool {
        if !Self::contains(from) || !Self::contains(to) {
            return false;
        }

        let min = from.min(to);
        let max = from.max(to);

        for x in min.x..=max.x {
            for y in min.y..=max.y {
                self.0[x as usize][y as usize] = true;
            }
        }

        true
    }

    pub fn is_masked(&self, pos: IVec2) -> Option<bool> {
        if Self::contains(pos) {
            Some(self.0[pos.x as usize][pos.y as usize])
        } else {
            None
        }
    }
}
