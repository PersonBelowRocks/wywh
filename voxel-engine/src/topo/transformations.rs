//! Utilities for transforming positions to and from different "vector spaces" (but maybe not really).
//! # Background
//! As you probably know, voxel engines store data in a way that easily maps the structure and layout
//! of the data into three-dimensional space. This eliminates the need for *a lot* of space partitioning
//! because we don't need to do a bunch of searches to find what data is at a certain position, making
//! reads and writes to data basically O(1). Obviously this comes with the tradeoff that now everything
//! is suddenly cuboid-shaped and it's *really hard* and *really annoying* to make things not cuboid
//! shaped (or at least behave like it's a cuboid in some way).
//!
//! But there's still a need for some space partitioning when it comes to voxels. This engine uses "chunks",
//! where the actual voxel data is stored in chunks and you have to load/unload entire chunks of voxel data.
//! There's also another level of partitioning below the chunk: subdivided blocks!
//! We want to be able to split up a block into microblocks for some extra detail and fanciness, but
//! we need to be able to describe a position within a subdivided block. This module has utilities to
//! simplify the process of moving between these different "spaces". You see, all these different constructs
//! (chunks, microblocks, full-blocks) are actually voxels. All the transformations between the spaces where
//! these constructs live are basically just translations and/or scaling.
//!
//! # The "spaces"
//! ### Localspace and Locality
//! Positions within a chunk are local. If chunks are 16x16x16 blocks then a local position at `[0, 0, 0]`
//! would be the block at the "minimum" corner of the chunk, and `[15, 15, 15]` would be the "maximum".
//! Positions can also exceed these limits. For example a position at `[16, 16, 16]` would describe the
//! "minimum" corner of an adjacent chunk.
//!
//! Locality can also be used to describe microblock positions within a subdivided block.
//!
//! We can say that the origin in localspace is at the "minimum" corner of whatever voxel-type structure
//! we're localized to.
//!
//! ### World
//! Positions with their origin at the "center" of the world are in "worldspace". This position is what
//! is most important to the player, and a lot of engine logic is spent on making sure that the player never
//! has to think about the other spaces.
//!
//! ### Subdivisions
//! It's often useful to, for example, address a microblock inside a chunk without thinking about the
//! block that it's contained inside. In this case we might want to treat a chunk as a big cube of
//! 64x64x64 blocks instead of 16x16x16. We can call such a space "microblock localspace" or "local microblock space"
//! since we are addressing *microblocks* that are *local* to a chunk. Or we could do the same but for
//! the entire world!

use bevy::math::{ivec3, IVec2, IVec3, IVec4};

/// Describes an integer vector like `IVec3` or `IVec2`.
pub trait IntegerVector<const SIZE: usize> {
    fn to_array(self) -> [i32; SIZE];
    fn from_array(arr: [i32; SIZE]) -> Self;
}

macro_rules! impl_integer_vector {
    ($t:ty, $size:literal) => {
        impl crate::topo::transformations::IntegerVector<$size> for $t {
            fn to_array(self) -> [i32; $size] {
                self.into()
            }

            fn from_array(arr: [i32; $size]) -> Self {
                Self::from(arr)
            }
        }
    };
}

impl_integer_vector!(IVec4, 4);
impl_integer_vector!(IVec3, 3);
impl_integer_vector!(IVec2, 2);

/// Calculate the "remainder" of `x / n^2`. It's not actually the remainder, and
/// this operation is not the same as, say, `rem_euclid` or `%` (at least I think so).
#[inline]
const fn rem_2_pow_n(x: i32, n: u32) -> i32 {
    let pow = 0b1 << n;
    x & ((pow - 1) as i32)
}

/// Calculate the floor of `x / n^2`.
#[inline]
const fn div_2_pow_n(x: i32, n: u32) -> i32 {
    x >> n as i32
}

/// The full-block dimensions of a chunk. The number of blocks in a chunk will be
/// `CHUNK_FULL_BLOCK_DIMS ^ 3`.
pub const CHUNK_FULL_BLOCK_DIMS: u32 = 16u32;
pub const CHUNK_FULL_BLOCK_DIMS_LOG2: u32 = CHUNK_FULL_BLOCK_DIMS.ilog2();

// We only want to deal with powers of two.
static_assertions::const_assert_eq!(1, CHUNK_FULL_BLOCK_DIMS.count_ones());
// The number must be trivially castable to an i32.
static_assertions::const_assert!((i32::MAX as u32) >= CHUNK_FULL_BLOCK_DIMS);

/// The microblock dimensions of a full-block. The number of microblocks in a
/// full-block will be `FULL_BLOCK_MICROBLOCK_DIMS ^ 3`.
pub const FULL_BLOCK_MICROBLOCK_DIMS: u32 = 4;
pub const FULL_BLOCK_MICROBLOCK_DIMS_LOG2: u32 = FULL_BLOCK_MICROBLOCK_DIMS.ilog2();

// We only want to deal with powers of two.
static_assertions::const_assert_eq!(1, FULL_BLOCK_MICROBLOCK_DIMS.count_ones());
// The number must be trivially castable to an i32.
static_assertions::const_assert!((i32::MAX as u32) >= FULL_BLOCK_MICROBLOCK_DIMS);

/// The microblock dimensions of a chunk. The number of microblocks in a chunk
/// will be `CHUNK_MICROBLOCK_DIMS ^ 3`.
pub const CHUNK_MICROBLOCK_DIMS: u32 = CHUNK_FULL_BLOCK_DIMS * FULL_BLOCK_MICROBLOCK_DIMS;
pub const CHUNK_MICROBLOCK_DIMS_LOG2: u32 = CHUNK_MICROBLOCK_DIMS.ilog2();

// We only want to deal with powers of two.
static_assertions::const_assert_eq!(1, CHUNK_MICROBLOCK_DIMS.count_ones());
// The number must be trivially castable to an i32.
static_assertions::const_assert!((i32::MAX as u32) >= CHUNK_MICROBLOCK_DIMS);

////////////////////////////////////////////////////////////////////////////////////
// To worldspace
////////////////////////////////////////////////////////////////////////////////////

/// World full-block position of a chunk's minimum corner from chunk position.
/// ### In
/// Chunk position
/// ### Out
/// World full-block position of the chunk position's corner
#[inline(always)]
pub fn chunkspace_to_worldspace_min<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| e * (CHUNK_FULL_BLOCK_DIMS as i32));
    T::from_array(arr)
}

/// World microblock position of a chunk's minimum corner from chunk position.
/// ### In
/// Chunk position
/// ### Out
/// World microblock position of the chunk position's corner
#[inline(always)]
pub fn chunkspace_to_mb_worldspace_min<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| e * (CHUNK_MICROBLOCK_DIMS as i32));
    T::from_array(arr)
}

////////////////////////////////////////////////////////////////////////////////////
// To chunkspace
////////////////////////////////////////////////////////////////////////////////////

/// Chunk position from world full-block position.
/// ### In
/// World full-block position
/// ### Out
/// World chunk position
#[inline(always)]
pub fn fb_worldspace_to_chunkspace<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| div_2_pow_n(e, CHUNK_FULL_BLOCK_DIMS_LOG2));
    T::from_array(arr)
}

/// Chunk position from world microblock position.
/// ### In
/// World microblock position
/// ### Out
/// World chunk position
#[inline(always)]
pub fn mb_worldspace_to_chunkspace<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| div_2_pow_n(e, CHUNK_MICROBLOCK_DIMS_LOG2));
    T::from_array(arr)
}

/// Local chunk position from local full-block position. In local chunkspace the chunk we're localized
/// to is at `[0, 0, 0]` and (as an example) the chunk above the localized chunk is at `[0, 1, 0]`
/// (assuming Y is height).
/// ### In
/// Local full-block position
/// ### Out
/// Local chunk position
#[inline(always)]
pub fn fb_localspace_to_local_chunkspace<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| div_2_pow_n(e, CHUNK_FULL_BLOCK_DIMS_LOG2));
    T::from_array(arr)
}

/// Local chunk position from local microblock position. In local chunkspace the chunk we're localized
/// to is at `[0, 0, 0]` and (as an example) the chunk above the localized chunk is at `[0, 1, 0]`
/// (assuming Y is height).
/// ### In
/// Local microblock position
/// ### Out
/// Local chunk position
#[inline(always)]
pub fn mb_localspace_to_local_chunkspace<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| div_2_pow_n(e, CHUNK_MICROBLOCK_DIMS_LOG2));
    T::from_array(arr)
}

////////////////////////////////////////////////////////////////////////////////////
// To localspace
////////////////////////////////////////////////////////////////////////////////////

/// Local full-block position from world full-block position.
/// ### In
/// World full-block position
/// ### Out
/// Local full-block position
#[inline(always)]
pub fn fb_worldspace_to_fb_localspace<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| rem_2_pow_n(e, CHUNK_FULL_BLOCK_DIMS_LOG2));
    T::from_array(arr)
}

/// In cases where you want to get the position of a full-block in a neighboring chunk from a position centered
/// on your localized chunk, this transformation might be useful as it essentially makes the position "wrap"
/// around.
/// ### In
/// Local full-block position (possibly outside of chunk bounds)
/// ### Out
/// Local full-block position (but within chunk bounds)
#[inline(always)]
pub fn fb_localspace_wrap<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| rem_2_pow_n(e, CHUNK_FULL_BLOCK_DIMS_LOG2));
    T::from_array(arr)
}

/// Local microblock position from world microblock position.
/// ### In
/// World microblock position
/// ### Out
/// Local microblock position
#[inline(always)]
pub fn mb_worldspace_to_mb_localspace<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| rem_2_pow_n(e, CHUNK_MICROBLOCK_DIMS_LOG2));
    T::from_array(arr)
}

/// In cases where you want to get the position of a microblock in a neighboring chunk from a position centered
/// on your localized chunk, this transformation might be useful as it essentially makes the position "wrap"
/// around.
/// ### In
/// Local microblock position (possibly outside of chunk bounds)
/// ### Out
/// Local microblock position (but within chunk bounds)
#[inline(always)]
pub fn mb_localspace_wrap<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| rem_2_pow_n(e, CHUNK_MICROBLOCK_DIMS_LOG2));
    T::from_array(arr)
}

/// Local full-block position from local microblock position.
/// ### In
/// Local microblock position
/// ### Out
/// Local full-block position
#[inline(always)]
pub fn mb_localspace_to_fb_localspace<const SIZE: usize, T>(input: T) -> T
where
    T: IntegerVector<{ SIZE }>,
{
    let mut arr = input.to_array();
    arr = arr.map(|e| div_2_pow_n(e, FULL_BLOCK_MICROBLOCK_DIMS_LOG2));
    T::from_array(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fb_localspace_to_local_chunkspace() {
        for x in 0..16 {
            for y in 0..16 {
                for z in 0..16 {
                    assert_eq!(
                        ivec3(0, 0, 0),
                        fb_localspace_to_local_chunkspace(ivec3(x, y, z))
                    );
                }
            }
        }

        for x in 16..32 {
            for y in 16..32 {
                for z in 16..32 {
                    assert_eq!(
                        ivec3(1, 1, 1),
                        fb_localspace_to_local_chunkspace(ivec3(x, y, z))
                    );
                }
            }
        }

        for x in 0..16 {
            for y in 16..32 {
                for z in 0..16 {
                    assert_eq!(
                        ivec3(0, 1, 0),
                        fb_localspace_to_local_chunkspace(ivec3(x, y, z))
                    );
                }
            }
        }
    }

    #[test]
    fn test_mb_localspace_to_local_chunkspace() {
        for x in 0..64 {
            for y in 0..64 {
                for z in 0..64 {
                    assert_eq!(
                        ivec3(0, 0, 0),
                        mb_localspace_to_local_chunkspace(ivec3(x, y, z))
                    );
                }
            }
        }

        for x in 64..128 {
            for y in 64..128 {
                for z in 64..128 {
                    assert_eq!(
                        ivec3(1, 1, 1),
                        mb_localspace_to_local_chunkspace(ivec3(x, y, z))
                    );
                }
            }
        }

        for x in 0..64 {
            for y in 64..128 {
                for z in 0..64 {
                    assert_eq!(
                        ivec3(0, 1, 0),
                        mb_localspace_to_local_chunkspace(ivec3(x, y, z))
                    );
                }
            }
        }
    }

    #[test]
    fn test_chunkspace_to_mb_worldspace_min() {
        let f = |x: i32, y: i32, z: i32| chunkspace_to_mb_worldspace_min(ivec3(x, y, z));

        assert_eq!(ivec3(0, 0, 0), f(0, 0, 0));
        assert_eq!(ivec3(0, 64, 0), f(0, 1, 0));
        assert_eq!(ivec3(0, -64, 0), f(0, -1, 0));
        assert_eq!(ivec3(0, -128, 0), f(0, -2, 0));
        assert_eq!(ivec3(0, 256, 0), f(0, 4, 0));
    }

    #[test]
    fn test_chunkspace_to_worldspace_min() {
        let f = |x: i32, y: i32, z: i32| chunkspace_to_worldspace_min(ivec3(x, y, z));

        assert_eq!(ivec3(0, 0, 0), f(0, 0, 0));
        assert_eq!(ivec3(0, 16, 0), f(0, 1, 0));
        assert_eq!(ivec3(0, -16, 0), f(0, -1, 0));
        assert_eq!(ivec3(0, -32, 0), f(0, -2, 0));
        assert_eq!(ivec3(0, 64, 0), f(0, 4, 0));
    }
}
