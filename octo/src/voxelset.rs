use bitvec::array::BitArray;
use bitvec::order::Lsb0;
use bitvec::view::BitView;
use glam::{IVec3, UVec3, ivec3};
use hashbrown::HashMap;
use hashbrown::hash_map::Entry;
use itertools::iproduct;
use slab::Slab;

use crate::voxelmap::Chunk;
use crate::{Region, div_2_pow_n, rem_2_pow_n};

/// Assert that a region bounded by a min and max position is valid to use in operations on a voxel set chunk.
#[track_caller]
#[inline]
fn assert_valid_vsc_region(pmin: [u8; 3], pmax: [u8; 3]) {
    for i in 0..3 {
        assert!(pmin[i] <= VoxelSetChunk::DIMS_U8);
        assert!(pmax[i] <= VoxelSetChunk::DIMS_U8);

        assert!(pmin[i] < pmax[i]);
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VoxelSetChunk([[u8; Self::DIMS_USIZE]; Self::DIMS_USIZE]);

impl VoxelSetChunk {
    /// The dimensions of a voxel set chunk along all axes.
    pub const DIMENSIONS: i32 = u8::BITS as _;
    /// Same as [`Self::DIMENSIONS`] but as a [`u8`] type.
    pub const DIMS_U8: u8 = Self::DIMENSIONS as _;
    /// Same as [`Self::DIMENSIONS`] but as a [`usize`] type.
    pub const DIMS_USIZE: usize = Self::DIMENSIONS as _;

    /// The minimum allowed position for indexing into a chunk.
    pub const MIN_POS: [u8; 3] = [0; 3];
    /// The maximum allowed position for indexing into a chunk.
    /// If any component in the array is greater than [`Self::DIMENSIONS`],
    /// then the array is illegal as an index position.
    pub const MAX_POS: [u8; 3] = [Self::DIMS_U8 - 1; 3];
    /// Same as [`Self::MAX_POS`], but as an [`IVec3`].
    pub const MAX_POS_IVEC3: IVec3 = IVec3::from_array([Self::DIMENSIONS - 1; 3]);

    /// An empty chunk. Created from [`VoxelSetChunk::empty()`]
    pub const EMPTY: Self = Self::empty();
    /// A filled chunk. Created from [`VoxelSetChunk::filled()`].
    pub const FILLED: Self = Self::filled();

    /// Create an empty voxel set chunk.
    #[must_use]
    #[inline]
    pub const fn empty() -> Self {
        Self([[0; Self::DIMS_USIZE]; Self::DIMS_USIZE])
    }

    /// Create a filled voxel set chunk.
    #[must_use]
    #[inline]
    pub const fn filled() -> Self {
        Self([[0xFF; Self::DIMS_USIZE]; Self::DIMS_USIZE])
    }

    /// Insert the given position into the chunk.
    ///
    /// `p` is the position in this chunk that we should insert.
    ///
    /// # Panics
    /// Will panic if for any element `n` in `p`: `p[n] >= VoxelSetChunk::CHUNK_DIMENSIONS`.
    #[inline]
    pub fn insert(&mut self, p: [u8; 3]) {
        let [p0, p1, p2] = p.map(usize::from);
        assert!((p1 as u8) < Self::DIMS_U8);
        let mask = 0b1u8 << (p1 as u8);

        let slot = &mut self.0[p0][p2];
        *slot = *slot | mask;
    }

    /// Set a region of positions at once.
    ///
    /// # Panics
    /// Will panic if any component of `pmin` is greater than or equal to the corresponding component of `pmax`.
    ///
    /// # Example
    /// ```rust
    /// # use octo::voxelset::VoxelSetChunk;
    ///
    /// let mut set = VoxelSetChunk::empty();
    /// set.insert_region([2, 1, 2], [6, 3, 5]);
    /// assert!(set.contains([2, 1, 2]));
    /// assert!(set.contains([2, 2, 2]));
    /// // The region is not inclusive of the maximum position.
    /// assert!(!set.contains([2, 3, 2]));
    /// // The maximum position is not included.
    /// assert!(!set.contains([6, 3, 5]));
    /// // All axes
    /// assert!(!set.contains([6, 2, 4]));
    /// assert!(!set.contains([5, 3, 4]));
    /// assert!(!set.contains([5, 2, 5]));
    /// // This position is one step below the maximum in all axes.
    /// assert!(set.contains([5, 2, 4]));
    /// ```
    #[inline(always)]
    pub fn insert_region(&mut self, pmin: [u8; 3], pmax: [u8; 3]) {
        assert_valid_vsc_region(pmin, pmax);

        let p1min = pmin[1];
        let p1max = pmax[1];

        let num_bits = p1max - p1min;
        let offset = p1min;

        let mask = (0xff >> (Self::DIMS_U8 - num_bits)) << offset;

        for (p0, p2) in iproduct!(pmin[0]..pmax[0], pmin[2]..pmax[2]) {
            let column = &mut self.0[p0 as usize][p2 as usize];
            *column = *column | mask;
        }
    }

    /// Remove the given position from the chunk.
    ///
    /// `p` is the position in this chunk that we should remove.
    ///
    /// # Panics
    /// Will panic if for any element `n` in `p`: `p[n] >= 8`.
    #[inline]
    pub fn remove(&mut self, p: [u8; 3]) {
        let [p0, p1, p2] = p.map(usize::from);
        assert!((p1 as u8) < Self::DIMS_U8);
        let mask = !(0b1u8 << (p1 as u8));

        let slot = &mut self.0[p0][p2];
        *slot = *slot & mask;
    }

    /// Remove a region of positions at once.
    ///
    /// # Panics
    /// Will panic if any component of `pmin` is greater than or equal to the corresponding component of `pmax`.
    ///
    /// # Example
    /// ```rust
    /// # use octo::voxelset::VoxelSetChunk;
    ///
    /// let mut set = VoxelSetChunk::empty();
    /// set.insert_region([0, 0, 0], [4, 4, 8]);
    /// set.remove_region([0, 0, 1], [4, 4, 7]);
    ///
    /// assert!(set.contains([0, 0, 0]));
    /// assert!(set.contains([0, 0, 7]));
    /// assert!(set.contains([3, 3, 0]));
    /// assert!(set.contains([3, 3, 7]));
    ///
    /// assert!(!set.contains([0, 0, 1]));
    /// assert!(!set.contains([0, 0, 6]));
    /// assert!(!set.contains([3, 3, 1]));
    /// assert!(!set.contains([3, 3, 6]));
    /// ```
    #[inline(always)]
    pub fn remove_region(&mut self, pmin: [u8; 3], pmax: [u8; 3]) {
        assert_valid_vsc_region(pmin, pmax);

        let p1min = pmin[1];
        let p1max = pmax[1];

        let num_bits = p1max - p1min;
        let offset = p1min;

        let mask = !(0xff >> (Self::DIMS_U8 - num_bits)) << offset;

        for (p0, p2) in iproduct!(pmin[0]..pmax[0], pmin[2]..pmax[2]) {
            let column = &mut self.0[p0 as usize][p2 as usize];
            *column = *column & mask;
        }
    }

    /// Check if the given position exists in this chunk.
    ///
    /// `p` is the position in this chunk that we should check.
    ///
    /// # Panics
    /// Will panic if for any element `n` in `p`: `p[n] >= 8`.
    ///
    /// # Example
    /// ```
    /// # use octo::voxelset::VoxelSetChunk;
    ///
    /// let mut chunk = VoxelSetChunk::empty();
    /// assert!(!chunk.contains([2, 5, 4]));
    /// chunk.insert([2, 5, 4]);
    /// assert!(chunk.contains([2, 5, 4]));
    /// assert!(!chunk.contains([2, 4, 4]));
    /// assert!(!chunk.contains([2, 6, 4]));
    /// ```
    #[inline]
    #[must_use]
    pub fn contains(&self, p: [u8; 3]) -> bool {
        let [p0, p1, p2] = p.map(usize::from);
        assert!((p1 as u8) < Self::DIMS_U8);
        let mask = 0b1u8 << (p1 as u8);

        let slot = self.0[p0][p2];
        (slot & mask) != 0
    }

    /// Check if this chunk contains the given region in its entirety.
    ///
    /// # Panics
    /// Will panic if any component of `pmin` is greater than or equal to the corresponding component of `pmax`.
    ///
    /// # Example
    /// ```rust
    /// # use octo::voxelset::VoxelSetChunk;
    ///
    /// let mut set = VoxelSetChunk::empty();
    /// set.insert_region([0, 0, 0], [4, 4, 8]);
    /// // set contains the region we just inserted
    /// assert!(set.contains_region([0, 0, 0], [4, 4, 8]));
    /// // set also contains subregions of that region
    /// assert!(set.contains_region([0, 0, 0], [2, 2, 2]));
    /// assert!(set.contains_region([2, 2, 2], [3, 3, 8]));
    /// // set does NOT contain regions that only partially overlap,
    /// // or regions that don't even overlap at all
    /// assert!(!set.contains_region([0, 0, 0], [4, 5, 8])); // partially overlaps
    /// assert!(!set.contains_region([0, 6, 0], [4, 8, 4])); // doesn't overlap at all
    /// ```
    #[inline(never)]
    #[must_use]
    pub fn contains_region(&self, pmin: [u8; 3], pmax: [u8; 3]) -> bool {
        assert_valid_vsc_region(pmin, pmax);

        let p1min = pmin[1];
        let p1max = pmax[1];

        let num_bits = p1max - p1min;
        let offset = p1min;

        let mask = (0xff >> (Self::DIMS_U8 - num_bits)) << offset;

        for (p0, p2) in iproduct!(pmin[0]..pmax[0], pmin[2]..pmax[2]) {
            let column = self.0[p0 as usize][p2 as usize];
            // the entire mask must be present in the column
            if column & mask != mask {
                return false;
            }
        }

        true
    }

    /// Returns the number of positions present in this chunk.
    /// This operation may be slightly costly so the result should be cached where possible.
    ///
    /// # Examples
    /// An empty chunk:
    /// ```rust
    /// # use octo::voxelset::VoxelSetChunk;
    ///
    /// let set = VoxelSetChunk::empty();
    /// assert_eq!(0, set.count());
    /// ```
    /// A filled chunk:
    /// ```rust
    /// # use octo::voxelset::VoxelSetChunk;
    ///
    /// let set = VoxelSetChunk::filled();
    /// assert_eq!(8 * 8 * 8, set.count());
    /// ```
    /// A single position:
    /// ```rust
    /// # use octo::voxelset::VoxelSetChunk;
    ///
    /// let mut set = VoxelSetChunk::empty();
    /// assert_eq!(0, set.count());
    /// set.insert([2, 7, 0]);
    /// assert_eq!(1, set.count());
    /// ```
    ///
    #[inline]
    #[must_use]
    pub fn count(&self) -> usize {
        let mut count = 0;

        for (p0, p2) in itertools::iproduct!(0..Self::DIMS_USIZE, 0..Self::DIMS_USIZE) {
            count += self.0[p0][p2].count_ones() as usize;
        }

        count
    }

    /// Check if this chunk is empty. Equivalent to `VoxelSetChunk::count() == 0`.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::EMPTY
    }

    /// Check if this chunk is filled. Equivalent to `VoxelSetChunk::count() == 512`.
    #[inline]
    #[must_use]
    pub fn is_filled(&self) -> bool {
        self == &Self::FILLED
    }

    #[inline]
    #[must_use]
    pub fn iter(&self) -> VoxelSetChunkIter<'_> {
        // we start iterating at this column
        let first_col = VoxelSetChunkColIter {
            col_y: 0,
            bits: BitArray::new(self.0[0][0]),
        };

        VoxelSetChunkIter {
            lx: 0,
            lz: 0,
            column: first_col,
            chunk: self,
        }
    }
}

/// Represents a column of a chunk in a voxel set which can be iterated through.
#[derive(Debug)]
struct VoxelSetChunkColIter {
    /// the current Y position in the column, used for indexing the bits
    col_y: u8,
    /// bits of the column
    bits: BitArray<u8>,
}

impl Iterator for VoxelSetChunkColIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        while self.col_y < VoxelSetChunk::DIMS_U8 as _ && !self.bits[self.col_y as usize] {
            self.col_y += 1;
        }

        let out = match self.col_y {
            ..VoxelSetChunk::DIMS_U8 => Some(self.col_y),
            _ => None,
        }?;

        // this column Y was present, so next iteration we need to check the Y after this one
        self.col_y += 1;

        Some(out)
    }
}

/// An iterator over positions in a [`VoxelSetChunk`]
pub struct VoxelSetChunkIter<'a> {
    /// local X of the column inside the chunk
    lx: u8,
    /// local Z of the column inside the chunk
    lz: u8,
    /// The current column inside the chunk
    column: VoxelSetChunkColIter,
    /// The chunk we're iterating through
    chunk: &'a VoxelSetChunk,
}

impl VoxelSetChunkIter<'_> {
    /// Advance to the next column in the chunk, returning its iterator.
    ///
    /// Returns [`None`] if no more columns are left.
    #[inline]
    fn advance_column(&mut self) -> Option<&mut VoxelSetChunkColIter> {
        self.lx += 1;
        if self.lx >= 8 {
            self.lx = 0;
            self.lz += 1;

            if self.lz >= 8 {
                self.lz = 0;
                return None;
            }
        }

        let bits = self.chunk.0[self.lx as usize][self.lz as usize];
        self.column = VoxelSetChunkColIter {
            col_y: 0,
            bits: BitArray::new(bits),
        };

        Some(&mut self.column)
    }
}

impl Iterator for VoxelSetChunkIter<'_> {
    type Item = [u8; 3];

    fn next(&mut self) -> Option<Self::Item> {
        let ly = loop {
            let Some(ly) = self.column.next() else {
                self.advance_column()?;
                continue;
            };

            break ly;
        };

        Some([self.lx, ly, self.lz])
    }
}

const DIMS_LOG2: u32 = VoxelSetChunk::DIMENSIONS.ilog2();

/// Get the chunk position containing the given gloal position.
#[inline]
fn chunk_pos(p: IVec3) -> IVec3 {
    p.to_array().map(|k| div_2_pow_n(k, DIMS_LOG2)).into()
}

/// Get the local position within the chunk of a given global position.
#[inline]
fn local_pos(p: IVec3) -> [u8; 3] {
    p.to_array().map(|k| rem_2_pow_n(k, DIMS_LOG2) as u8)
}

/// Split position into the position of the [`VoxelSetChunk`] containing it, and the local position within that chunk.
/// Used to first get a position's chunk, then refer to the position's value inside that chunk.
#[inline]
fn chunk_and_local(p: IVec3) -> (IVec3, [u8; 3]) {
    let chunk: IVec3 = p.to_array().map(|k| div_2_pow_n(k, DIMS_LOG2)).into();
    let local = p.to_array().map(|k| rem_2_pow_n(k, DIMS_LOG2) as u8);

    (chunk, local)
}

/// Calculate the global position of a chunk and a local offset within that chunk.
/// Be careful that the local position not exceed [`VoxelSetChunk::DIMENSIONS`], you may get weird results if it does.
#[inline]
fn global_from_chunk_and_local(chunk_pos: IVec3, local: [u8; 3]) -> IVec3 {
    let base: IVec3 = chunk_pos.to_array().map(|k| k << (DIMS_LOG2 as i32)).into();
    let local: IVec3 = local.map(i32::from).into();

    base + local
}

/// Get a region which entirely contains the chunk at the given position.
#[inline]
fn chunk_region(chunk_pos: IVec3) -> Region {
    let min = global_from_chunk_and_local(chunk_pos, VoxelSetChunk::MIN_POS);
    let max = global_from_chunk_and_local(chunk_pos, VoxelSetChunk::MAX_POS);

    // must be inclusive since we want to contain MAX_POS as it's valid
    Region::new_inclusive(min, max)
}

/// A set of voxel positions. Like a hashset but supports more specialized operations.
#[derive(Clone, Default)]
pub struct VoxelSet {
    chunks: HashMap<IVec3, usize, rustc_hash::FxBuildHasher>,
    slab: Slab<VoxelSetChunk>,
}

impl VoxelSet {
    /// Create a new and empty voxel set.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a position to the set.
    #[inline]
    pub fn insert(&mut self, pos: IVec3) {
        let (chunk_pos, local_pos) = chunk_and_local(pos);

        match self.chunks.entry(chunk_pos) {
            Entry::Occupied(entry) => {
                let chunk_index = *entry.get();
                let chunk = self.slab.get_mut(chunk_index).unwrap();
                chunk.insert(local_pos)
            }
            Entry::Vacant(entry) => {
                let mut chunk = VoxelSetChunk::empty();
                chunk.insert(local_pos);

                entry.insert(self.slab.insert(chunk));
            }
        }
    }

    /// Add a region of voxels to the set. This operation is a no-op if the provided region is degenerate (since degenerate regions have no volume).
    ///
    /// This is significantly faster than looping over the region and adding its positions individually.
    #[inline]
    pub fn insert_region(&mut self, region: Region) {
        if region.is_degenerate() {
            // region has zero volume, so there's nothing for us to do
            return;
        }

        let min_chunk = chunk_pos(region.min());
        let max_chunk = chunk_pos(region.max_contained());

        for chunk_pos in iproduct!(
            min_chunk.x..=max_chunk.x,
            min_chunk.y..=max_chunk.y,
            min_chunk.z..=max_chunk.z
        )
        .map(IVec3::from)
        {
            let chunk_entry = self.chunks.entry(chunk_pos);

            // this is the region of space that this chunk contains
            let chunk_region = chunk_region(chunk_pos);
            // this region is the part of the current chunk which overlaps with the provided region.
            // if the entire chunk's region overlaps with the provided region, this will be equal to the chunk's region.
            let overlapping_chunk_region = Region::intersection(chunk_region, region)
                .expect("should always intersect since this is derived from the parameter");

            debug_assert!(
                !overlapping_chunk_region.is_degenerate(),
                "this intersection should not be degenerate; we have the ability to ensure it never is"
            );

            if overlapping_chunk_region == chunk_region {
                // the entire chunk is contained in the provided region, just set the chunk all at once
                match chunk_entry {
                    // replace existing chunk with a filled one
                    Entry::Occupied(chunk_index) => {
                        self.slab[*chunk_index.get()] = VoxelSetChunk::filled()
                    }
                    // insert new filled chunk
                    Entry::Vacant(entry) => {
                        entry.insert(self.slab.insert(VoxelSetChunk::filled()));
                    }
                }

                continue;
            }

            // in this case, we need to be a bit more precise when working inside a chunk and only insert a specific region in a chunk
            let local_min = local_pos(overlapping_chunk_region.min());
            let local_max = local_pos(overlapping_chunk_region.max_contained());

            match chunk_entry {
                Entry::Occupied(chunk_index) => {
                    // work on the existing region
                    self.slab[*chunk_index.get()].insert_region(local_min, local_max);
                }
                Entry::Vacant(entry) => {
                    // create new empty chunk, insert the appropriate region, add the chunk to the slab, and insert the index into the vacant entry
                    let mut chunk = VoxelSetChunk::empty();
                    chunk.insert_region(local_min, local_max);
                    entry.insert(self.slab.insert(chunk));
                }
            }
        }
    }

    /// Remove a position from the set.
    #[inline]
    pub fn remove(&mut self, pos: IVec3) {
        let (chunk_pos, local_pos) = chunk_and_local(pos);

        let Entry::Occupied(entry) = self.chunks.entry(chunk_pos) else {
            return;
        };

        let chunk_index = *entry.get();
        let chunk = self.slab.get_mut(chunk_index).unwrap();
        chunk.remove(local_pos);

        if chunk.is_empty() {
            self.slab.remove(chunk_index);
            entry.remove();
        }
    }

    /// Remove a region of voxels from the set. This operation is a no-op if the provided region is degenerate (since degenerate regions have no volume).
    ///
    /// This is significantly faster than looping over the region and removing its positions individually.
    #[inline]
    pub fn remove_region(&mut self, region: Region) {
        if region.is_degenerate() {
            // region has zero volume, so there's nothing for us to do
            return;
        }

        let min_chunk = chunk_pos(region.min());
        let max_chunk = chunk_pos(region.max_contained());

        for chunk_pos in iproduct!(
            min_chunk.x..=max_chunk.x,
            min_chunk.y..=max_chunk.y,
            min_chunk.z..=max_chunk.z
        )
        .map(IVec3::from)
        {
            let Entry::Occupied(chunk_index_entry) = self.chunks.entry(chunk_pos) else {
                // since we're removing we can skip chunks that don't exist in the set
                continue;
            };

            let chunk_index = *chunk_index_entry.get();

            // this is the region of space that this chunk contains
            let chunk_region = chunk_region(chunk_pos);
            // this region is the part of the current chunk which overlaps with the provided region.
            // if the entire chunk's region overlaps with the provided region, this will be equal to the chunk's region.
            let overlapping_chunk_region = Region::intersection(chunk_region, region)
                .expect("should always intersect since this is derived from the parameter");

            debug_assert!(
                !overlapping_chunk_region.is_degenerate(),
                "this intersection should not be degenerate; we have the ability to ensure it never is"
            );

            if overlapping_chunk_region == chunk_region {
                // the entire chunk is contained in the provided region, so just remove the whole thing
                self.slab.remove(chunk_index);
                chunk_index_entry.remove();

                continue;
            }

            // in this case, we need to be a bit more precise when working inside a chunk and only remove a specific region in a chunk
            let local_min = local_pos(overlapping_chunk_region.min());
            let local_max = local_pos(overlapping_chunk_region.max_contained());

            // work on existing region
            self.slab[chunk_index].remove_region(local_min, local_max);

            // if the chunk is empty after this, remove it
            if self.slab[chunk_index].is_empty() {
                self.slab.remove(chunk_index);
                chunk_index_entry.remove();
            }
        }
    }

    /// Check if the position is present in this set.
    #[inline]
    #[must_use]
    pub fn contains(&self, pos: IVec3) -> bool {
        let (chunk_pos, local_pos) = chunk_and_local(pos);

        self.chunks
            .get(&chunk_pos)
            .map(|&chunk_index| &self.slab[chunk_index])
            .is_some_and(|chunk| chunk.contains(local_pos))
    }

    /// Check if the entire region is fully contained within this set.
    /// That means that all the positions in the region are present in this set.
    ///
    /// # Warning
    /// If the region is degenerate, this will return [`false`].
    /// Degenerate regions contain no positions, therefore we have nothing in the region to check for.
    #[inline]
    #[must_use]
    pub fn contains_region(&self, region: Region) -> bool {
        if region.is_degenerate() {
            return false;
        }

        let min_chunk = chunk_pos(region.min());
        let max_chunk = chunk_pos(region.max_contained());

        for chunk_pos in iproduct!(
            min_chunk.x..=max_chunk.x,
            min_chunk.y..=max_chunk.y,
            min_chunk.z..=max_chunk.z
        )
        .map(IVec3::from)
        {
            let Some(chunk_index) = self.chunks.get(&chunk_pos).copied() else {
                // this chunk didn't exist in the set, so the region was not contained
                return false;
            };

            // this is the region of space that this chunk contains
            let chunk_region = chunk_region(chunk_pos);
            // this region is the part of the current chunk which overlaps with the provided region.
            // if the entire chunk's region overlaps with the provided region, this will be equal to the chunk's region.
            let overlapping_chunk_region = Region::intersection(chunk_region, region)
                .expect("should always intersect since this is derived from the parameter");

            debug_assert!(
                !overlapping_chunk_region.is_degenerate(),
                "this intersection should not be degenerate; we have the ability to ensure it never is"
            );

            if overlapping_chunk_region == chunk_region {
                // this entire chunk is present in the region, therefore the entire chunk must be filled in order for the region to be contained
                if !self.slab[chunk_index].is_filled() {
                    return false;
                } else {
                    continue;
                }
            }

            // in this case, we need to be a bit more precise and check the actual contents of a chunk
            let local_min = local_pos(overlapping_chunk_region.min());
            let local_max = local_pos(overlapping_chunk_region.max_contained());

            if !self.slab[chunk_index].contains_region(local_min, local_max) {
                return false;
            }
        }

        true
    }

    /// Iterate over all voxels present in this set in a random order.
    #[inline]
    #[must_use]
    pub fn iter(&self) -> VoxelSetIter<'_> {
        VoxelSetIter {
            chunks: self.chunks.iter(),
            slab: &self.slab,
            current_chunk: None,
        }
    }
}

pub struct VoxelSetIter<'a> {
    chunks: hashbrown::hash_map::Iter<'a, IVec3, usize>,
    slab: &'a Slab<VoxelSetChunk>,
    current_chunk: Option<(IVec3, VoxelSetChunkIter<'a>)>,
}

impl VoxelSetIter<'_> {
    /// Set the current chunk to `self.chunks.next()` and return its position.
    ///
    /// Will return `None` if the chunk iterator is exhausted.
    fn advance_chunk(&mut self) -> Option<IVec3> {
        let (&next_chunk_pos, &next_chunk_index) = self.chunks.next()?;

        let chunk_iter = self.slab[next_chunk_index].iter();
        self.current_chunk = Some((next_chunk_pos, chunk_iter));
        Some(next_chunk_pos)
    }
}

impl Iterator for VoxelSetIter<'_> {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        let (chunk_pos, local_pos) = loop {
            let Some((chunk_pos, chunk_iter)) = &mut self.current_chunk else {
                // this branch will only happen once per iterator, and will just initialize the first chunk iterator.
                self.advance_chunk()?;
                continue;
            };

            let Some(local_pos) = chunk_iter.next() else {
                // nothing left in the current chunk so we advance to the next one.
                self.advance_chunk()?;
                continue;
            };

            break (*chunk_pos, local_pos);
        };

        let local_pos: IVec3 = local_pos.map(i32::from).into();
        Some((chunk_pos * 8) + local_pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::ivec3;
    use hashbrown::HashSet;

    #[test]
    fn test_iter() {
        let mut hashset = HashSet::<IVec3>::new();
        let mut voxelset = VoxelSet::new();

        let mut insert = |pos: IVec3| {
            hashset.insert(pos);
            voxelset.insert(pos);
        };

        insert(ivec3(0, 0, 0));
        insert(ivec3(0, 1, 0));
        insert(ivec3(0, 0, 1));
        insert(ivec3(1, 0, 0));
        insert(ivec3(1, 1, 1));
        insert(ivec3(8, 8, 8));
        insert(ivec3(0, 0, 8));
        insert(ivec3(0, 8, 0));
        insert(ivec3(12, 0, 0));
        insert(ivec3(0, 0, 100));
        insert(ivec3(0, 1, 0));
        insert(ivec3(0, 2, 0));
        insert(ivec3(0, 3, 0));
        insert(ivec3(0, 6, 0));

        for pos in voxelset.iter() {
            assert!(hashset.contains(&pos));
            hashset.remove(&pos);
        }

        assert!(hashset.is_empty());
    }

    #[test]
    fn test_single() {
        let mut set = VoxelSet::new();

        set.insert(ivec3(0, 0, 0));
        set.insert(ivec3(0, 1, 0));
        set.insert(ivec3(10, 17, 3));
        set.insert(ivec3(-1, 0, 1));

        assert!(set.contains(ivec3(0, 0, 0)));
        assert!(set.contains(ivec3(0, 1, 0)));
        assert!(set.contains(ivec3(10, 17, 3)));
        assert!(set.contains(ivec3(-1, 0, 1)));
    }

    #[test]
    #[should_panic]
    fn test_insert_max() {
        let mut set = VoxelSet::new();
        set.insert(ivec3(i32::MAX, 0, 0));
    }

    #[test]
    fn test_region() {
        let mut set = VoxelSet::new();

        set.insert_region(Region::new([0, 0, 0], [5, 5, 5]));
        assert!(set.contains(ivec3(0, 0, 0)));
        assert!(!set.contains(ivec3(5, 5, 5)));
        assert!(!set.contains(ivec3(4, 5, 4)));
        assert!(set.contains(ivec3(4, 4, 4)));
        assert!(!set.contains(ivec3(2, -4, 2)));

        assert!(!set.contains_region(Region::new([0, 0, 0], [5, 5, 5])));
        assert!(set.contains_region(Region::new([0, 0, 0], [4, 4, 4])));
        assert!(set.contains_region(Region::new([1, 1, 2], [3, 2, 3])));

        assert!(!set.contains_region(Region::new([0, -5, 0], [2, 2, 3])));

        set.insert_region(Region::new([0, 0, 5], [5, 5, 9]));

        assert!(set.contains_region(Region::new([0, 0, 0], [4, 4, 4])));
        assert!(set.contains_region(Region::new([0, 0, 0], [4, 4, 5])));
        assert!(set.contains_region(Region::new([0, 0, 0], [4, 4, 6])));
        assert!(set.contains_region(Region::new([0, 0, 0], [4, 4, 7])));
        assert!(set.contains_region(Region::new([0, 0, 0], [4, 4, 8])));

        set.remove_region(Region::new([0, 0, 0], [5, 5, 5]));
    }

    #[test]
    fn test_chunk_region() {
        let mut set = VoxelSetChunk::empty();
        set.insert_region([0, 0, 0], [4, 4, 8]);
        // the set contains the region we just inserted
        assert!(set.contains_region([0, 0, 0], [4, 4, 8]));

        // the set also contains subregions of that region
        assert!(set.contains_region([0, 0, 0], [2, 2, 2]));
        assert!(set.contains_region([2, 2, 2], [3, 3, 8]));

        // the set does NOT contain regions that only partially overlap,
        // or regions that don't even overlap at all
        assert!(!set.contains_region([0, 0, 0], [4, 5, 8])); // partially overlaps
        assert!(!set.contains_region([0, 6, 0], [4, 8, 4])); // doesn't overlap at all
    }
}
