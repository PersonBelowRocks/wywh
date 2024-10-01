use std::ops::Range;

use glam::{ivec3, IVec3, UVec3};
use hashbrown::{hash_map::Entry, HashMap};

/// The maximum allowed size of a chunk in a voxel set.
pub const MAX_CHUNK_SIZE: usize = u16::MAX as _;

/// A chunk of integer points in a voxel set.
#[derive(Clone)]
struct VoxelSetChunk<const S: usize> {
    num: u32,
    // TODO: this is incorrect, needs to be S ^ 3
    data: [u8; S],
}

impl<const S: usize> Default for VoxelSetChunk<S> {
    fn default() -> Self {
        Self {
            num: 0,
            data: [0u8; S],
        }
    }
}

impl<const S: usize> VoxelSetChunk<S> {
    /// Create a voxel set chunk containing no voxels.
    #[inline]
    pub fn empty() -> Self {
        Self {
            num: 0,
            data: [0u8; S],
        }
    }

    /// Create a voxel set chunk filled with voxels.
    #[inline]
    pub fn filled() -> Self {
        Self {
            num: (S * S * S) as _,
            data: [u8::MAX; S],
        }
    }

    /// Convert a 3d position into a 1d index into the data of this chunk. Note that
    /// the index must be divided by the number of bits in a byte (8 bits) in order to correspond
    /// to an actual value in the data array. I.E., the index is a index to a bit in the chunk's data, not a byte!
    #[inline]
    pub fn to_1d(p: UVec3) -> u32 {
        let max = S as u32;
        let [p0, p1, p2] = p.to_array();

        (p2 * max * max) + (p1 * max) + (p0)
    }

    /// Set the voxel at the given position to be present or absent.
    /// `p` must be in bounds for the given chunk size.
    #[inline]
    pub fn set(&mut self, p: UVec3, value: bool) {
        let p_1d = Self::to_1d(p);

        let byte = &mut self.data[(p_1d / u8::BITS) as usize];
        let bit = (p_1d % u8::BITS) as u8;

        let new_byte = match value {
            true => *byte | (0b1 << bit),
            false => *byte & !(0b1 << bit),
        };

        if new_byte > *byte {
            // If the new byte is greater than the old one, we have added an extra 1 somewhere which means
            // that we inserted a new position into this chunk.
            self.num += 1;
        } else if new_byte < *byte {
            // If the new byte is smaller than the old one, then we have removed a 1 from the byte (replacing it with a 0),
            // which means that we removed a position from this chunk.
            self.num -= 1;
        }

        // If neither of the above conditions are true, that means that the bytes are the same and no changes were made.

        *byte = new_byte;
    }

    /// Check if the given voxel is present in this chunk.
    /// `p` must be in bounds for the given chunk size.
    #[inline]
    pub fn contains(&self, p: UVec3) -> bool {
        let p_1d = Self::to_1d(p);

        let byte = self.data[(p_1d / u8::BITS) as usize];
        let bit = (p_1d % u8::BITS) as u8;

        (byte & (0b1 << bit)) != 0
    }

    /// The number of voxels present in this chunk.
    #[inline]
    pub fn num(&self) -> u32 {
        self.num
    }

    /// Returns `true` if there are no voxels present in this chunk.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.num() == 0
    }
}

/// Asserts that the given chunk dimensions are valid.
///
/// Panics if:
/// - `CHUNK_DIMS` is 0.
/// - `CHUNK_DIMS` is not a power of 2.
/// - `CHUNK_DIMS` is greater than [`MAX_CHUNK_SIZE`].
pub fn assert_chunk_dims_valid(chunk_dims: usize) {
    assert!(chunk_dims != 0, "chunk dimensions must not be 0");
    assert!(
        chunk_dims.count_ones() == 1,
        "chunk dimensions must be a power of 2"
    );
    assert!(
        chunk_dims <= (u32::MAX as usize),
        "chunk dimensions must be less than or equal to u32::MAX"
    );
}

pub fn floor_div(a: i32, b: i32) -> i32 {
    a / b
}

pub fn ceil_div(a: i32, b: i32) -> i32 {
    (a + b - 1) / b
}

/// A set of 3D integer points (or voxels).
pub struct VoxelSet<const CHUNK_DIMS: usize> {
    data: HashMap<IVec3, VoxelSetChunk<CHUNK_DIMS>, rustc_hash::FxBuildHasher>,
}

impl<const CHUNK_DIMS: usize> VoxelSet<CHUNK_DIMS> {
    /// Create a new empty voxel set.
    ///
    /// # Panics
    /// Panics if:
    /// - `CHUNK_DIMS` is 0.
    /// - `CHUNK_DIMS` is not a power of 2.
    /// - `CHUNK_DIMS` is greater than [`MAX_CHUNK_SIZE`].
    pub fn new() -> Self {
        assert_chunk_dims_valid(CHUNK_DIMS);

        Self {
            data: HashMap::with_hasher(rustc_hash::FxBuildHasher::default()),
        }
    }

    /// Create a new chunk set with all the chunks in the given range being set.
    ///
    /// # Panics
    /// Panics if:
    /// - `CHUNK_DIMS` is 0.
    /// - `CHUNK_DIMS` is not a power of 2.
    /// - `CHUNK_DIMS` is greater than [`MAX_CHUNK_SIZE`].
    #[inline]
    pub fn from_region(region: Range<IVec3>) -> Self {
        let mut set = Self::new();

        let min = region.start.min(region.end);
        let max = region.start.max(region.end);
        let dims = max - min;

        let num_chunks = dims
            .div_euclid(IVec3::splat(CHUNK_DIMS as i32))
            .element_product();
        set.data.reserve(num_chunks as usize);

        let min_chunk = ivec3(
            floor_div(min.x, CHUNK_DIMS as i32),
            floor_div(min.y, CHUNK_DIMS as i32),
            floor_div(min.z, CHUNK_DIMS as i32),
        );

        let max_chunk = ivec3(
            ceil_div(min.x, CHUNK_DIMS as i32),
            ceil_div(min.y, CHUNK_DIMS as i32),
            ceil_div(min.z, CHUNK_DIMS as i32),
        );

        todo!()
    }

    /// Set the status of a voxel in the voxel set.
    #[inline]
    pub fn set(&mut self, p: IVec3, value: bool) {
        let chunk_pos = p.div_euclid(IVec3::splat(CHUNK_DIMS as i32));
        let local_pos = p.rem_euclid(IVec3::splat(CHUNK_DIMS as i32)).as_uvec3();

        match self.data.entry(chunk_pos) {
            Entry::Occupied(mut entry) => {
                let chunk = entry.get_mut();
                chunk.set(local_pos, value);

                // If the chunk is empty, remove it
                if chunk.is_empty() {
                    entry.remove();
                }
            }
            // Only bother inserting a new chunk if we're actually making a voxel present in the set.
            // Otherwise we would just be inserting an empty chunk which does not change the observable behaviour of the set.
            Entry::Vacant(entry) if value => {
                let mut chunk = VoxelSetChunk::empty();
                chunk.set(local_pos, value);
                entry.insert(chunk);
            }
            _ => (),
        }
    }

    /// Returns `true` if the given voxel is present in the set.
    #[inline]
    pub fn contains(&self, p: IVec3) -> bool {
        let chunk_pos = p.div_euclid(IVec3::splat(CHUNK_DIMS as i32));
        let local_pos = p.rem_euclid(IVec3::splat(CHUNK_DIMS as i32)).as_uvec3();

        self.data
            .get(&chunk_pos)
            .map(|chunk| chunk.contains(local_pos))
            .unwrap_or(false)
    }
}
