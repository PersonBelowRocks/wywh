use bitvec::array::BitArray;
use bitvec::order::Lsb0;
use bitvec::view::BitView;
use glam::{IVec3, UVec3, ivec3};
use hashbrown::HashMap;
use hashbrown::hash_map::Entry;
use slab::Slab;

use crate::voxelmap::Chunk;
use crate::{Region, div_2_pow_n, rem_2_pow_n};

/// Assert that a region bounded by a min and max position is valid to use in operations on a voxel set chunk.
#[track_caller]
#[inline]
fn assert_valid_vsc_region(pmin: [u8; 3], pmax: [u8; 3]) {
    for i in 0..3 {
        assert!(pmin[i] <= 8);
        assert!(pmax[i] <= 8);

        assert!(pmin[i] < pmax[i]);
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VoxelSetChunk([[u8; 8]; 8]);

impl VoxelSetChunk {
    /// An empty chunk. Created from [`VoxelSetChunk::empty()`]
    pub const EMPTY: Self = Self::empty();
    /// A filled chunk. Created from [`VoxelSetChunk::filled()`].
    pub const FILLED: Self = Self::filled();

    /// Create an empty voxel set chunk.
    #[must_use]
    #[inline]
    pub const fn empty() -> Self {
        Self([[0; 8]; 8])
    }

    /// Create a filled voxel set chunk.
    #[must_use]
    #[inline]
    pub const fn filled() -> Self {
        Self([[0xFF; 8]; 8])
    }

    /// Insert the given position into the chunk.
    ///
    /// `p` is the position in this chunk that we should insert.
    ///
    /// # Panics
    /// Will panic if for any element `n` in `p`: `p[n] >= 8`.
    #[inline]
    pub fn insert(&mut self, p: [u8; 3]) {
        let [p0, p1, p2] = p.map(usize::from);
        assert!((p1 as u8) < 8);
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

        let mask = (0xff >> (8 - num_bits)) << offset;

        for (p0, p2) in itertools::iproduct!(pmin[0]..pmax[0], pmin[2]..pmax[2]) {
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
        assert!((p1 as u8) < 8);
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

        let mask = !(0xff >> (8 - num_bits)) << offset;

        for (p0, p2) in itertools::iproduct!(pmin[0]..pmax[0], pmin[2]..pmax[2]) {
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
        assert!((p1 as u8) < 8);
        let mask = 0b1u8 << (p1 as u8);

        let slot = self.0[p0][p2];
        (slot & mask) != 0
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

        for (p0, p2) in itertools::iproduct!(0..8usize, 0..8usize) {
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
        while self.col_y < 8 && !self.bits[self.col_y as usize] {
            self.col_y += 1;
        }

        let out = match self.col_y {
            ..8 => Some(self.col_y),
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

#[inline]
fn chunk_and_local(p: IVec3) -> (IVec3, [u8; 3]) {
    const DIMS_LOG2: u32 = 8u32.ilog2();

    let chunk: IVec3 = p.to_array().map(|k| div_2_pow_n(k, DIMS_LOG2)).into();
    let local = p.to_array().map(|k| rem_2_pow_n(k, DIMS_LOG2) as u8);

    (chunk, local)
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

    /// Add a region of voxels to the set.
    ///
    /// This is significantly faster than looping over the region and adding its positions individually.
    #[inline]
    pub fn insert_region(&mut self, region: Region) {
        todo!()
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

    /// Remove a region of voxels from the set.
    #[inline]
    pub fn remove_region(&mut self, region: Region) {
        todo!()
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
    #[inline]
    #[must_use]
    pub fn contains_region(&self, region: Region) -> bool {
        todo!()
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
}
