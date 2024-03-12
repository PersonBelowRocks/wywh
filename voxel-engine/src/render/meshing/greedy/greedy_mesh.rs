use bevy::{math::ivec2, prelude::IVec2};

use crate::{
    topo::{block::SubdividedBlock, chunk::Chunk},
    util::SquareArray,
};

#[derive(Copy, Clone, Default, Hash, PartialEq, Eq, Debug)]
pub(crate) enum TileMask {
    Full,
    #[default]
    None,
    Mosaic,
}

#[derive(Clone)]
pub(crate) struct ChunkSliceMask {
    tiles: SquareArray<{ Chunk::USIZE }, TileMask>,
    microblocks: SquareArray<{ Chunk::SUBDIVIDED_CHUNK_USIZE }, bool>,
}

impl ChunkSliceMask {
    pub fn new() -> Self {
        Self {
            tiles: [[TileMask::default(); Chunk::USIZE]; Chunk::USIZE],
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

        let min = IVec2::min(pos1, pos2);
        let max = IVec2::max(pos1, pos2);

        for x in min.x..=max.x {
            for y in min.y..=max.y {
                self.tiles[x as usize][y as usize] = TileMask::Full;
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

        let min_rem = min_mb.rem_euclid(SubdividedBlock::SUBDIVS_VEC);
        let max_rem = max_mb.rem_euclid(SubdividedBlock::SUBDIVS_VEC);

        let min = min_mb.div_euclid(SubdividedBlock::SUBDIVS_VEC);
        let max = max_mb.div_euclid(SubdividedBlock::SUBDIVS_VEC);

        if min_rem == IVec2::ZERO && max_rem == IVec2::ZERO {
            self.mask_region_inclusive(min, max);
        } else {
            // regular mask on the completely covered tiles, sort of a low-res mask
            self.mask_region_inclusive(min + IVec2::ONE, max - IVec2::ONE);

            // fill the microblock tiles, sort of a high-res mask
            for x in min_mb.x..=max_mb.x {
                for y in min_mb.y..=max_mb.y {
                    self.microblocks[x as usize][y as usize] = true;
                }
            }

            // mark the border tiles as being mosaic, so lookups should be done in the microblock mask
            for x in min.x..=max.x {
                self.tiles[x as usize][min.y as usize] = TileMask::Mosaic;
                self.tiles[x as usize][max.y as usize] = TileMask::Mosaic;
            }
            for y in min.y..=max.y {
                self.tiles[min.x as usize][y as usize] = TileMask::Mosaic;
                self.tiles[max.x as usize][y as usize] = TileMask::Mosaic;
            }
        }

        true
    }

    pub fn is_masked_mb(&self, pos: IVec2) -> Option<bool> {
        if !Self::contains_mb(pos) {
            return None;
        }

        let tile = pos.div_euclid(SubdividedBlock::SUBDIVS_VEC);

        Some(match self.tiles[tile.x as usize][tile.y as usize] {
            TileMask::Full => true,
            TileMask::None => false,
            TileMask::Mosaic => self.microblocks[pos.x as usize][pos.y as usize],
        })
    }

    pub fn is_masked(&self, pos: IVec2) -> Option<bool> {
        Some(match self.get_tile_mask(pos)? {
            TileMask::Full => true,
            TileMask::Mosaic | TileMask::None => false,
        })
    }

    pub fn get_tile_mask(&self, pos: IVec2) -> Option<TileMask> {
        if !Self::contains(pos) {
            None
        } else {
            Some(self.tiles[pos.x as usize][pos.y as usize])
        }
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

        assert_eq!(Some(TileMask::Full), mask.get_tile_mask(ivec2(1, 1)));
        assert_eq!(Some(TileMask::Full), mask.get_tile_mask(ivec2(2, 1)));

        assert_eq!(Some(TileMask::Mosaic), mask.get_tile_mask(ivec2(0, 1)));
        assert_eq!(Some(TileMask::Mosaic), mask.get_tile_mask(ivec2(3, 1)));
        assert_eq!(Some(TileMask::Mosaic), mask.get_tile_mask(ivec2(1, 2)));
        assert_eq!(Some(TileMask::Mosaic), mask.get_tile_mask(ivec2(1, 0)));
        assert_eq!(Some(TileMask::Mosaic), mask.get_tile_mask(ivec2(0, 0)));

        assert_eq!(Some(TileMask::None), mask.get_tile_mask(ivec2(1, 3)));
        assert_eq!(Some(TileMask::None), mask.get_tile_mask(ivec2(4, 1)));

        assert!(mask.is_masked_mb(ivec2(2, 3)).unwrap());
        assert!(mask.is_masked_mb(ivec2(9, 6)).unwrap());
    }
}
