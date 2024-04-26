use bevy::{math::ivec2, prelude::IVec2};

use crate::{
    topo::{block::SubdividedBlock, world::Chunk},
    util::SquareArray,
};

#[derive(Clone)]
pub(crate) struct ChunkSliceMask {
    microblocks: SquareArray<{ Chunk::SUBDIVIDED_CHUNK_USIZE }, bool>,
}

impl ChunkSliceMask {
    pub fn new() -> Self {
        Self {
            microblocks: [[false; Chunk::SUBDIVIDED_CHUNK_USIZE]; Chunk::SUBDIVIDED_CHUNK_USIZE],
        }
    }

    pub fn contains(pos: IVec2) -> bool {
        pos.cmpge(ivec2(0, 0)).all() && pos.cmplt(ivec2(Chunk::SIZE, Chunk::SIZE)).all()
    }

    pub fn contains_mb(pos: IVec2) -> bool {
        Self::contains(pos.div_euclid(IVec2::splat(SubdividedBlock::SUBDIVISIONS)))
    }

    pub fn mask_region_inclusive(&mut self, pos1: IVec2, pos2: IVec2) -> bool {
        if !Self::contains(pos1) || !Self::contains(pos2) {
            return false;
        }

        let min = IVec2::min(pos1, pos2) * SubdividedBlock::SUBDIVISIONS;
        let max = (IVec2::max(pos1, pos2) + IVec2::ONE) * SubdividedBlock::SUBDIVISIONS;

        for x in min.x..max.x {
            for y in min.y..max.y {
                self.microblocks[x as usize][y as usize] = true;
            }
        }

        true
    }

    pub fn mask_mb_region_inclusive(&mut self, pos1: IVec2, pos2: IVec2) -> bool {
        if !Self::contains_mb(pos1) || !Self::contains_mb(pos2) {
            return false;
        }

        let min_mb = IVec2::min(pos1, pos2);
        let max_mb = IVec2::max(pos1, pos2);

        for x in min_mb.x..=max_mb.x {
            for y in min_mb.y..=max_mb.y {
                self.microblocks[x as usize][y as usize] = true;
            }
        }

        true
    }

    pub fn is_masked_mb(&self, pos: IVec2) -> Option<bool> {
        if !Self::contains_mb(pos) {
            return None;
        }

        Some(self.microblocks[pos.x as usize][pos.y as usize])
    }

    pub fn is_masked(&self, pos: IVec2) -> Option<bool> {
        if !Self::contains(pos) {
            return None;
        }

        for x in 0..4 {
            for y in 0..4 {
                let p = ivec2(x, y) + (pos * SubdividedBlock::SUBDIVISIONS);
                if !self.microblocks[p.x as usize][p.y as usize] {
                    return Some(false);
                }
            }
        }

        Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /*
    |-----------------------------------|
    |[][][][]|[][][][]|[][][][]|[][][][]| <- pos_mb = (16, 12), pos = (4, 3) (max pos)
    |[][][][]|[][][][]|[][][][]|[][][][]|    i.e., pos_mb = pos * 4
    |[][][][]|[][][][]|[][][][]|P2[][][]|
    2[][][][]|[][][][]|[][][][]|[][][][]|
    |-----------------------------------|
    |[][][][]|[][][][]|[][][][]|[][][][]|
    |[][][][]|[][][][]|[][][][]|[][][][]| } y_mb = 12, y = 3
    |[][][][]|[][][][]|[][][][]|[][][][]|
    1[][][][]|[][][][]|[][][][]|[][][][]|
    |-----------------------------------|
    |[][]P1[]|[][][][]|[][][][]|[][][][]|
    |[][][][]|[][][][]|[][][][]|[][][][]|
    |[][][][]|[][][][]|[][][][]|[][][][]|
    0[][][][]|[][][][]|[][][][]|[][][][]|
    |0-------|1-------|2-------|3-------|
                x_mb = 16, x = 4

    P1 = (2, 3) (min)
    P2 = (13, 9) (max)
    */

    #[test]
    fn mask_logic() {
        let mut mask = ChunkSliceMask::new();

        assert!(mask.mask_mb_region_inclusive(ivec2(2, 3), ivec2(13, 9)));
        assert!(!mask.is_masked(ivec2(0, 1)).unwrap());
        assert!(mask.is_masked(ivec2(1, 1)).unwrap());
        assert!(mask.is_masked(ivec2(2, 1)).unwrap());
        assert!(!mask.is_masked(ivec2(3, 1)).unwrap());

        assert!(mask.is_masked_mb(ivec2(2, 3)).unwrap());
        assert!(mask.is_masked_mb(ivec2(9, 6)).unwrap());
    }

    #[test]
    fn mask_single_microblock() {
        let mut mask = ChunkSliceMask::new();
        assert!(mask.mask_mb_region_inclusive(ivec2(8, 8), ivec2(8, 8)));

        assert!(mask.is_masked_mb(ivec2(8, 8)).unwrap());
        assert!(!mask.is_masked_mb(ivec2(8, 9)).unwrap());
        assert!(!mask.is_masked_mb(ivec2(9, 9)).unwrap());
        assert!(!mask.is_masked_mb(ivec2(9, 8)).unwrap());
    }
}
