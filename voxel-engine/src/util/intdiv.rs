use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::topo::{
    block::SubdividedBlock,
    world::{Chunk, ChunkPos},
};

#[inline]
pub const fn rem_euclid_2_pow_n(x: i32, n: u32) -> i32 {
    let pow = 0b1 << n;
    x & ((pow - 1) as i32)
}

#[inline]
pub const fn floored_div_2_pow_n(x: i32, n: u32) -> i32 {
    x >> n as i32
}

#[inline]
pub const fn microblock_to_full_block(mb: IVec2) -> IVec2 {
    ivec2(
        floored_div_2_pow_n(mb.x, SubdividedBlock::SUBDIVISIONS_LOG2),
        floored_div_2_pow_n(mb.y, SubdividedBlock::SUBDIVISIONS_LOG2),
    )
}

#[inline]
pub const fn microblock_to_full_block_3d(mb: IVec3) -> IVec3 {
    ivec3(
        floored_div_2_pow_n(mb.x, SubdividedBlock::SUBDIVISIONS_LOG2),
        floored_div_2_pow_n(mb.y, SubdividedBlock::SUBDIVISIONS_LOG2),
        floored_div_2_pow_n(mb.z, SubdividedBlock::SUBDIVISIONS_LOG2),
    )
}

#[inline]
pub const fn microblock_to_subdiv_pos(mb: IVec2) -> IVec2 {
    ivec2(
        rem_euclid_2_pow_n(mb.x, SubdividedBlock::SUBDIVISIONS_LOG2),
        rem_euclid_2_pow_n(mb.y, SubdividedBlock::SUBDIVISIONS_LOG2),
    )
}

#[inline]
pub const fn microblock_to_subdiv_pos_3d(mb: IVec3) -> IVec3 {
    ivec3(
        rem_euclid_2_pow_n(mb.x, SubdividedBlock::SUBDIVISIONS_LOG2),
        rem_euclid_2_pow_n(mb.y, SubdividedBlock::SUBDIVISIONS_LOG2),
        rem_euclid_2_pow_n(mb.z, SubdividedBlock::SUBDIVISIONS_LOG2),
    )
}

#[inline]
pub const fn ws_to_chunk_pos(ws_pos: IVec3) -> ChunkPos {
    ChunkPos::new(
        floored_div_2_pow_n(ws_pos.x, Chunk::SIZE_LOG2),
        floored_div_2_pow_n(ws_pos.y, Chunk::SIZE_LOG2),
        floored_div_2_pow_n(ws_pos.z, Chunk::SIZE_LOG2),
    )
}

// TODO: make this const
#[inline]
pub fn chunk_pos_to_ws(chunk_pos: ChunkPos) -> IVec3 {
    chunk_pos.as_ivec3() * Chunk::SIZE
}

#[cfg(test)]
mod tests {
    use crate::util::floored_div_2_pow_n;

    use super::rem_euclid_2_pow_n;

    #[test]
    pub fn test_rem_euclid_2_pow_n() {
        let f = |x: i32| -> i32 { rem_euclid_2_pow_n(x, 4) };

        assert_eq!(0, f(16));
        assert_eq!(15, f(15));
        assert_eq!(15, f(-1));
        assert_eq!(0, f(0));
        assert_eq!(1, f(1));
        assert_eq!(6, f(6));
        assert_eq!(4, f(20));
        assert_eq!(12, f(-4));
    }

    #[test]
    pub fn test_floored_div_2_pow_n() {
        let f = |x: i32| -> i32 { floored_div_2_pow_n(x, 4) };

        assert_eq!(0, f(0));
        assert_eq!(0, f(1));
        assert_eq!(0, f(8));
        assert_eq!(0, f(9));
        assert_eq!(0, f(15));
        assert_eq!(1, f(16));
        assert_eq!(1, f(19));
        assert_eq!(-1, f(-1));
        assert_eq!(-1, f(-8));
        assert_eq!(-1, f(-16));
        assert_eq!(-2, f(-17));
    }
}
