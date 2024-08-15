use nda::Array3;
use std::any::type_name;
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T1};
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
use std::mem;

/// Trait for values that can be used in a [`SubdividedStorage`].
///
/// # Safety
/// Implementors must never use the highest bit in a [`u32`] for anything, as that bit is used
/// internally by the storage for tracking whether index entries point to subdivided palette entries or not.
pub unsafe trait SubdividableValue: Copy + Eq {
    /// Convert bits to this type. The first bit in the provided [`u32`] will always be `0`.
    fn from_bits_31(bits: u32) -> Self;

    /// Convert this type to a [`u32`] with no high bit.
    fn to_bits_31(self) -> u32;
}

macro_rules! subdiv_uint {
    ($t:ty) => {
        unsafe impl SubdividableValue for $t {
            fn from_bits_31(bits: u32) -> Self {
                bits as $t
            }

            fn to_bits_31(self) -> u32 {
                (self as u32) & !crate::subdiv::SSIndexEntry::<$t>::MASK
            }
        }
    };
}

subdiv_uint!(u32);
subdiv_uint!(u16);
subdiv_uint!(u8);

/// An index level entry of a subdivided storage.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct SSIndexEntry<T: SubdividableValue>(u32, PhantomData<T>);

impl<T: SubdividableValue> Debug for SSIndexEntry<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SSIndexEntry<{}>({:#01x})", type_name::<T>(), self.0)
    }
}

impl<T: SubdividableValue> SSIndexEntry<T> {
    /// Bitmask that targets the highest bit in the raw index entry value. This bit indicates if
    /// the entry points to a subdivided palette entry or if it contains the value locally.
    const MASK: u32 = (0b1 << (u32::BITS - 1));

    /// Create an index entry from a value.
    #[inline]
    pub fn from_value(value: T) -> Self {
        Self(value.to_bits_31(), PhantomData)
    }

    /// Create an index entry from an index.
    #[inline]
    pub fn from_index(index: u32) -> Self {
        let value = index | Self::MASK;
        Self(value, PhantomData)
    }

    /// Whether this index points to a subdivided entry or not.
    #[inline]
    pub fn points_to_subdivided(self) -> bool {
        (self.0 & Self::MASK) != 0
    }

    /// Get the index as a [`usize`].
    #[inline]
    pub fn as_usize(self) -> usize {
        (self.0 & !Self::MASK) as usize
    }

    /// Gets the value in the index.
    #[inline]
    pub fn as_value(self) -> T {
        T::from_bits_31(self.0 & !Self::MASK)
    }
}

/// A palette level entry of a subdivided storage. Contains a `SD*SD*SD` array of microblocks.
#[derive(Clone)]
pub struct SSPaletteEntry<const SD: usize, T: SubdividableValue>([[[T; SD]; SD]; SD]);

impl<const SD: usize, T: SubdividableValue> SSPaletteEntry<SD, T> {
    /// Creates a new palette entry filled with the given value.
    #[inline]
    pub fn new(filling: T) -> Self {
        Self([[[filling; SD]; SD]; SD])
    }

    /// Hint to the CPU to load the entire palette entry into cache
    /// Currently not properly tested and benchmarked so it should not be used.
    #[inline(always)]
    pub fn prefetch(&self) {
        // Assume a cache line is 64 bytes
        const CACHE_LINE: usize = 64;

        fn prefetch_ptr<K>(p: *const K) {
            let p = unsafe { mem::transmute::<*const K, *const i8>(p) };
            unsafe { _mm_prefetch::<{ _MM_HINT_T1 }>(p) };
        }

        let max_offset = (SD * SD * SD);
        let step_by = CACHE_LINE / size_of::<T>();
        for lane in (0..max_offset).step_by(step_by) {
            let root = &self.0[0][0][0] as *const T;
            // SAFETY: we're not reading from this pointer at any point so we can do whatever we want
            let lane_ptr = unsafe { root.add(lane) };

            prefetch_ptr::<T>(lane_ptr);
        }
    }

    /// Gets the value at a given index, returning [`None`] if the index is out of bounds.
    #[inline(always)]
    pub fn get(&self, index: [u8; 3]) -> Option<T> {
        let [i0, i1, i2] = index.map(usize::from);

        self.0.get(i0)?.get(i1)?.get(i2).copied()
    }

    /// Same as [`Self::get`] but does no bounds checking.
    ///
    /// # Safety
    /// The given index must be within the bounds of this palette entry
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: [u8; 3]) -> T {
        let [i0, i1, i2] = index.map(usize::from);

        unsafe { *self.0.get_unchecked(i0).get_unchecked(i1).get_unchecked(i2) }
    }

    /// Sets a value at the given index.
    ///
    /// # Panics
    /// Will panic if the index is out of bounds.
    #[inline(always)]
    pub fn set(&mut self, index: [u8; 3], value: T) {
        let [i0, i1, i2] = index.map(usize::from);
        self.0[i0][i1][i2] = value;
    }

    /// Sets a value at the given index.
    ///
    /// # Safety
    /// The given index must be within the bounds of this palette entry
    #[inline(always)]
    pub unsafe fn set_unchecked(&mut self, index: [u8; 3], value: T) {
        let [i0, i1, i2] = index.map(usize::from);
        unsafe {
            *self
                .0
                .get_unchecked_mut(i0)
                .get_unchecked_mut(i1)
                .get_unchecked_mut(i2) = value;
        }
    }

    /// Returns the first value (value at `[0, 0, 0]`)
    #[inline(always)]
    pub fn first(&self) -> T {
        self.0[0][0][0]
    }

    /// Test if all the values in this palette entry are equal.
    #[inline(always)]
    pub fn all_equal(&self) -> bool {
        let first = self.first();

        self.0
            .as_flattened()
            .as_flattened()
            .iter()
            .all(|e| e == &first)
    }
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum SubdivAccessError {
    #[error("{0:?} is out of bounds for the subdivided storage")]
    OutOfBounds([u8; 3]),
    #[error("Entry {0:#01x} at {1:?} is not a full block")]
    NonFullBlock(u32, [u8; 3]),
}

/// Stores a mixture of full blocks and microblocks in a more efficient way than just one big 3D array
#[derive(Clone)]
pub struct SubdividedStorage<const D: usize, const SD: usize, T: SubdividableValue> {
    indices: Array3<SSIndexEntry<T>>,
    sdiv_palette: Vec<SSPaletteEntry<SD, T>>,
}

impl<const D: usize, const SD: usize, T: SubdividableValue> SubdividedStorage<D, SD, T> {
    pub fn map_mb_indices<F: FnMut([u8; 3])>(mut f: F) {
        let d = D as u8;
        let sd = SD as u8;

        // All full-block indices
        for p0 in 0..d {
            for p1 in 0..d {
                for p2 in 0..d {
                    // Every microblock in this full block
                    for mb0 in (p0 * sd)..((p0 * sd) + sd) {
                        for mb1 in (p1 * sd)..((p1 * sd) + sd) {
                            for mb2 in (p2 * sd)..((p2 * sd) + sd) {
                                // TODO: should this be called in reverse?
                                f([mb0, mb1, mb2])
                            }
                        }
                    }
                }
            }
        }
    }

    /// Create a new subdivided storage filled with the provided value.
    ///
    /// # Panics
    /// Will panic if the product of the generic parameters `D` and `SD` is greater than [`u8::MAX`]
    #[inline]
    pub fn new(value: T) -> Self {
        assert!(
            D * SD <= u8::MAX as usize,
            "total dimensions exceed u8::MAX, which is not allowed"
        );
        let initial_entry_index = SSIndexEntry::from_value(value);

        Self {
            indices: Array3::from_elem((D, D, D), initial_entry_index),
            sdiv_palette: Vec::new(),
        }
    }

    /// Create a new subdivided storage filled with the provided value and with a palette of the given
    /// capacity.
    ///
    /// # Panics
    /// Will panic if the product of the generic parameters `D` and `SD` is greater than [`u8::MAX`]
    #[inline]
    pub fn with_capacity(value: T, capacity: usize) -> Self {
        assert!(
            D * SD <= u8::MAX as usize,
            "total dimensions exceed u8::MAX, which is not allowed"
        );
        let initial_entry_index = SSIndexEntry::from_value(value);

        Self {
            indices: Array3::from_elem((D, D, D), initial_entry_index),
            sdiv_palette: Vec::with_capacity(capacity),
        }
    }

    /// Get an entry of the storage's indices, returning [`None`] if the provided index is out of bounds.
    #[inline]
    pub fn get_entry(&self, index: [u8; 3]) -> Option<SSIndexEntry<T>> {
        self.indices.get(index.map(usize::from)).copied()
    }

    /// Get a mutable reference to an entry of the storage's indices,
    /// returning [`None`] if the provided index is out of bounds.
    #[inline]
    pub fn get_entry_mut(&mut self, index: [u8; 3]) -> Option<&mut SSIndexEntry<T>> {
        self.indices.get_mut(index.map(usize::from))
    }

    /// Get an index-level value from this storage, returning an error if the provided index is out of
    /// bounds or the entry at the index is subdivided. Will never query the palette for microblock values.
    /// Use [`Self::get_mb`] to get microblocks.
    #[inline]
    pub fn get(&self, index: [u8; 3]) -> Result<T, SubdivAccessError> {
        let entry = self
            .get_entry(index)
            .ok_or(SubdivAccessError::OutOfBounds(index))?;

        if entry.points_to_subdivided() {
            Err(SubdivAccessError::NonFullBlock(entry.0, index))
        } else {
            Ok(entry.as_value())
        }
    }

    /// Get a value from this storage, returning an error if the given index is out of bounds. This
    /// function operates on the microblock level rather than the index level.
    #[inline(always)]
    pub fn get_mb(&self, mb_index: [u8; 3]) -> Result<T, SubdivAccessError> {
        let index = mb_index_to_index(mb_index, SD as u8);
        let entry = self
            .get_entry(index)
            .ok_or(SubdivAccessError::OutOfBounds(index))?;

        if entry.points_to_subdivided() {
            let palette_index = entry.as_usize();
            let palette_entry = &self.sdiv_palette[palette_index];

            let local_index = mb_index_to_local_mb_index(mb_index, SD as u8);
            debug_assert!(local_index.iter().all(|c| *c < SD as u8));

            // SAFETY: 'mb_index_to_local_mb_index' can't return anything larger than 'SD'
            let value = unsafe { palette_entry.get_unchecked(local_index) };
            Ok(value)
        } else {
            Ok(entry.as_value())
        }
    }

    /// Set a value at the index level for this storage. If the entry at the index pointed to a subdivided
    /// palette entry, that entry will be leaked and must be manually removed by calling [`Self::cleanup`].
    /// Returns an error if the provided index is out of bounds.
    #[inline]
    pub fn set(&mut self, index: [u8; 3], value: T) -> Result<(), SubdivAccessError> {
        let entry = self
            .get_entry_mut(index)
            .ok_or(SubdivAccessError::OutOfBounds(index))?;

        *entry = SSIndexEntry::from_value(value);
        Ok(())
    }

    /// Set a value in this storage at the microblock level, returning an error if the index is out of bounds.
    /// Will automatically create and allocate a new palette entry if one doesn't already exist.
    #[inline]
    pub fn set_mb(&mut self, mb_index: [u8; 3], value: T) -> Result<(), SubdivAccessError> {
        let index = mb_index_to_index(mb_index, SD as u8);
        let new_index = self.sdiv_palette.len() as u32;
        let entry = self
            .get_entry_mut(index)
            .ok_or(SubdivAccessError::OutOfBounds(index))?;

        if entry.points_to_subdivided() {
            let palette_index = entry.as_usize();
            let palette_entry = &mut self.sdiv_palette[palette_index];

            let local_index = mb_index_to_local_mb_index(mb_index, SD as u8);
            debug_assert!(local_index.iter().all(|c| *c < SD as u8));

            // SAFETY: 'mb_index_to_local_mb_index' can't return anything larger than 'SD'
            unsafe {
                palette_entry.set_unchecked(local_index, value);
            }

            Ok(())
        } else {
            let old_value = entry.as_value();

            // Return early if we're setting the same value that already existed here
            if value == old_value {
                return Ok(());
            }

            *entry = SSIndexEntry::from_index(new_index);

            let mut palette_entry = SSPaletteEntry::new(old_value);
            let local_index = mb_index_to_local_mb_index(mb_index, SD as u8);
            palette_entry.set(local_index, value);

            self.sdiv_palette.push(palette_entry);

            Ok(())
        }
    }

    /// Clean up and merge values in the palette of this storage. Should be called after heavy writing to
    /// the storage to clean up potentially inaccessible palette entries. Merging is when values in a palette
    /// entry are combined into an index entry if they're all equal.
    #[inline]
    pub fn cleanup(&mut self) {
        let dims = D as u8;
        let mut new_indices = self.indices.clone();
        let mut new_palette = Vec::with_capacity(self.sdiv_palette.len());

        for i2 in 0..dims {
            for i1 in 0..dims {
                for i0 in 0..dims {
                    let index = [i0, i1, i2];
                    let entry = new_indices.get_mut(index.map(usize::from)).unwrap();

                    if entry.points_to_subdivided() {
                        let palette_entry = &self.sdiv_palette[entry.as_usize()];

                        if palette_entry.all_equal() {
                            *entry = SSIndexEntry::from_value(palette_entry.first());
                        } else {
                            let new_palette_entry_index = new_palette.len() as u32;
                            new_palette.push(palette_entry.clone());
                            *entry = SSIndexEntry::from_index(new_palette_entry_index);
                        }
                    }
                }
            }
        }

        self.indices = new_indices;
    }
}

#[inline(always)]
pub const fn mb_index_to_index(mb_index: [u8; 3], sdims: u8) -> [u8; 3] {
    let sdims_log2 = sdims.trailing_zeros();
    let [i0, i1, i2] = mb_index;

    [i0 >> sdims_log2, i1 >> sdims_log2, i2 >> sdims_log2]
}

#[inline(always)]
pub const fn mb_index_to_local_mb_index(mb_index: [u8; 3], sdims: u8) -> [u8; 3] {
    let sdims_log2 = sdims.trailing_zeros();
    let pow = 0b1 << sdims_log2;
    let [i0, i1, i2] = mb_index;

    [i0 & (pow - 1), i1 & (pow - 1), i2 & (pow - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io() {
        let mut s = SubdividedStorage::<16, 4, u32>::new(0);

        s.set([0, 0, 0], 42).unwrap();
        s.set([15, 15, 15], 43).unwrap();
        s.set([8, 8, 8], 1337).unwrap();
        s.set([9, 9, 9], 1337).unwrap();
        s.set([10, 10, 10], 404).unwrap();

        assert_eq!(42, s.get([0, 0, 0]).unwrap());
        assert_eq!(43, s.get([15, 15, 15]).unwrap());
        assert_eq!(1337, s.get([8, 8, 8]).unwrap());
        assert_eq!(1337, s.get([9, 9, 9]).unwrap());
        assert_eq!(404, s.get([10, 10, 10]).unwrap());
    }

    #[test]
    fn mb_io() {
        let mut s = SubdividedStorage::<16, 4, u32>::new(0);

        s.set([0, 0, 0], 42).unwrap();
        s.set_mb([63, 63, 63], 43).unwrap();
        s.set_mb([31, 31, 31], 1337).unwrap();
        s.set_mb([28, 28, 28], 1337).unwrap();
        s.set_mb([10, 10, 10], 404).unwrap();

        assert_eq!(
            42,
            s.get([0, 0, 0]).inspect_err(|e| println!("{e}")).unwrap()
        );
        assert_eq!(42, s.get_mb([0, 0, 0]).unwrap());
        assert_eq!(42, s.get_mb([3, 3, 3]).unwrap());

        assert_eq!(43, s.get_mb([63, 63, 63]).unwrap());

        assert_eq!(1337, s.get_mb([31, 31, 31]).unwrap());
        assert_eq!(1337, s.get_mb([28, 28, 28]).unwrap());
    }

    #[test]
    fn map_mb_indices() {
        type Storage = SubdividedStorage<16, 4, u32>;
        let mut storage = Storage::new(0);

        Storage::map_mb_indices(|index| {
            storage.set_mb(index, 10).unwrap();
        });

        for p0 in 0..64 {
            for p1 in 0..64 {
                for p2 in 0..64 {
                    assert_eq!(10, storage.get_mb([p0, p1, p2]).unwrap());
                }
            }
        }
    }

    #[test]
    fn cleanup() {
        const BASE: u8 = 16;

        fn suite(s: &SubdividedStorage<16, 4, u32>) {
            assert_eq!(42, s.get_mb([0, 0, 0]).unwrap());
            assert_eq!(43, s.get_mb([1, 1, 1]).unwrap());

            for x in 0..8 {
                for y in 0..8 {
                    for z in 0..8 {
                        let i = [BASE + x, BASE + y, BASE + z];
                        assert_eq!(1337, s.get_mb(i).unwrap());
                    }
                }
            }

            assert_eq!(1002, s.get_mb([0, 12, 0]).unwrap());
        }

        let mut s = SubdividedStorage::<16, 4, u32>::new(0);

        s.set_mb([0, 0, 0], 42).unwrap();
        s.set_mb([1, 1, 1], 43).unwrap();

        for x in 0..8 {
            for y in 0..8 {
                for z in 0..8 {
                    let i = [BASE + x, BASE + y, BASE + z];
                    s.set_mb(i, 1337).unwrap();
                }
            }
        }

        s.set_mb([0, 12, 0], 1001).unwrap();
        s.set([0, 3, 0], 1002).unwrap();

        suite(&s);

        s.cleanup();

        suite(&s);
    }
}
