use hashbrown::HashMap;
use smallvec::SmallVec;
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
pub unsafe trait SubdividableValue: Copy + Eq + std::hash::Hash {
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
pub struct SSPaletteEntry<const SD: usize, T: SubdividableValue> {
    data: [[[T; SD]; SD]; SD],
    cached_hash: Option<u64>,
}

impl<const SD: usize, T: SubdividableValue> SSPaletteEntry<SD, T> {
    /// Creates a new palette entry filled with the given value.
    #[inline]
    pub fn new(filling: T) -> Self {
        Self {
            data: [[[filling; SD]; SD]; SD],
            cached_hash: None,
        }
    }

    /// Get the cached hash for this entry, or compute the value if there's none cached.
    #[inline]
    pub fn cached_hash_compute(&self, state: &ahash::RandomState) -> u64 {
        self.cached_hash
            .unwrap_or_else(|| state.hash_one(self.data))
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

        let max_offset = SD * SD * SD;
        let step_by = CACHE_LINE / size_of::<T>();
        for lane in (0..max_offset).step_by(step_by) {
            let root = &self.data[0][0][0] as *const T;
            // SAFETY: we're not reading from this pointer at any point so we can do whatever we want
            let lane_ptr = unsafe { root.add(lane) };

            prefetch_ptr::<T>(lane_ptr);
        }
    }

    /// Gets the value at a given index, returning [`None`] if the index is out of bounds.
    #[inline(always)]
    pub fn get(&self, index: [u8; 3]) -> Option<T> {
        let [i0, i1, i2] = index.map(usize::from);

        self.data.get(i0)?.get(i1)?.get(i2).copied()
    }

    /// Same as [`Self::get`] but does no bounds checking.
    ///
    /// # Safety
    /// The given index must be within the bounds of this palette entry
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: [u8; 3]) -> T {
        let [i0, i1, i2] = index.map(usize::from);

        unsafe {
            *self
                .data
                .get_unchecked(i0)
                .get_unchecked(i1)
                .get_unchecked(i2)
        }
    }

    /// Sets a value at the given index.
    ///
    /// # Panics
    /// Will panic if the index is out of bounds.
    #[inline(always)]
    pub fn set(&mut self, index: [u8; 3], value: T) {
        let [i0, i1, i2] = index.map(usize::from);
        self.data[i0][i1][i2] = value;
        self.cached_hash = None;
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
                .data
                .get_unchecked_mut(i0)
                .get_unchecked_mut(i1)
                .get_unchecked_mut(i2) = value;
        }
        self.cached_hash = None;
    }

    /// Returns the first value (value at `[0, 0, 0]`)
    #[inline(always)]
    pub fn first(&self) -> T {
        self.data[0][0][0]
    }

    /// Test if all the values in this palette entry are equal.
    #[inline(always)]
    pub fn all_equal(&self) -> bool {
        let first = self.first();

        self.data
            .as_flattened()
            .as_flattened()
            .iter()
            .all(|e| e == &first)
    }
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum SubdivAccessError {
    #[error("{0:?} is out of bounds for the storage")]
    OutOfBounds([u8; 3]),
    #[error("Entry {0:#01x} at {1:?} is not a full block")]
    NonFullBlock(u32, [u8; 3]),
}

/// Error encountered when getting a reference *(mutable or immutable)* to an entry in a storage's palette.
#[derive(thiserror::Error, Debug, Clone)]
pub enum PaletteEntryError {
    /// The index was out of bounds for the storage.
    #[error("{0:?} is out of bounds for the storage")]
    OutOfBounds([u8; 3]),
    /// The entry at the given index was not subdivided and therefore had no palette entry.
    #[error("Entry {0:#01x} at {1:?} is not subdivided, so there is no palette entry for it")]
    NotSubdivided(u32, [u8; 3]),
    /// The storage is currently deflated and therefore palette entries cannot be mutated,
    /// so you cannot get a mutable reference to an entry within the palette.
    ///
    /// See [`SubdivPaletteKind`] and [`SubdividedStorage::deflate()`] for more information.
    #[error("Cannot get a mutable reference for a palette entry if the palette is deflated")]
    DeflatedStorage,
}

/// Describes whether the palette for a subdividable storage is inflated or deflated.
/// See the docs on the variants for a description of each kind and what it implies for the storage.
///
/// The different kinds only really affect how writing to a storage should be handled, the read logic
/// should be identical in both palette kinds.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum SubdivPaletteKind {
    /// In an inflated state, the palette has duplicates of its elements.
    /// Each subdivided value in a storage will have its own unique index in the palette.
    /// This uses a lot of memory since there will often be duplicates of subdivided values, but
    /// it will be extremely fast to write to the palette in this state.
    Inflated,
    /// In a deflated state, the palette has no duplicates of its elements (or at least less duplicates).
    /// The variant's [`usize`] field describes the length of the palette were it to be inflated again.
    /// This is very useful for preallocating a vector with a capacity that will fit all the items in an inflated
    /// palette. Writing directly to subdivided values is not possible in a deflated state, since other
    /// values may be pointing to the same palette entry. In order to write to a storage with a deflated palette
    /// it must first be inflated.
    Deflated(usize),
}

/// Stores a mixture of full blocks and microblocks in a more efficient way than just one big 3D array
#[derive(Clone)]
pub struct SubdividedStorage<const D: usize, const SD: usize, T: SubdividableValue> {
    /// The indices into the palette, or the values themselves if they're not subdivided.
    indices: [[[SSIndexEntry<T>; D]; D]; D],
    /// The palette, may be inflated or deflated depending on [`Self::palette_kind`].
    sdiv_palette: Vec<SSPaletteEntry<SD, T>>,
    /// The kind of palette currently being used by this storage.
    palette_kind: SubdivPaletteKind,
    /// The random state used during deflation
    random_state: ahash::RandomState,
}

impl<const D: usize, const SD: usize, T: SubdividableValue> SubdividedStorage<D, SD, T> {
    // TODO: get rid of this and make a flexible 3D iterator type
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
            indices: [[[initial_entry_index; D]; D]; D],
            sdiv_palette: Vec::new(),
            palette_kind: SubdivPaletteKind::Inflated,
            random_state: ahash::RandomState::new(),
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
            indices: [[[initial_entry_index; D]; D]; D],
            sdiv_palette: Vec::with_capacity(capacity),
            palette_kind: SubdivPaletteKind::Inflated,
            random_state: ahash::RandomState::new(),
        }
    }

    /// Whether the palette of this storage is inflated or not.
    /// See [`SubdivPaletteKind`] for more information.
    #[inline]
    pub fn is_inflated(&self) -> bool {
        matches!(self.palette_kind, SubdivPaletteKind::Inflated)
    }

    /// Whether the palette of this storage is deflated or not.
    /// See [`SubdivPaletteKind`] for more information.
    #[inline]
    pub fn is_deflated(&self) -> bool {
        matches!(self.palette_kind, SubdivPaletteKind::Deflated(_))
    }

    /// Get the length of the palette. This is not the number of unique elements but rather the
    /// length of the internal vector where all palette entries are stored. If called after a
    /// [`SubdividedStorage::deflate()`] call then this should be the number of unique entries in the palette.
    #[inline]
    pub fn palette_len(&self) -> usize {
        self.sdiv_palette.len()
    }

    /// Get a reference to the palette entry at the given index. Returns an error if:
    /// - Index is out of bounds.
    /// - Index is not subdivided and therefore has no palette entry.
    ///
    /// See [`PaletteEntryError`] for more information.
    #[inline]
    pub fn get_palette_entry(
        &self,
        index: [u8; 3],
    ) -> Result<&SSPaletteEntry<SD, T>, PaletteEntryError> {
        let entry = self
            .get_entry(index)
            .ok_or(PaletteEntryError::OutOfBounds(index))?;

        if !entry.points_to_subdivided() {
            return Err(PaletteEntryError::NotSubdivided(entry.0, index));
        }

        let palette_index = entry.as_usize();
        Ok(&self.sdiv_palette[palette_index])
    }

    /// Get a mutable reference to the palette entry at the given index. Returns an error if:
    /// - Index is out of bounds.
    /// - Index is not subdivided and therefore has no palette entry.
    /// - The storage is currently deflated and therefore cannot be mutated.
    ///
    /// See [`PaletteEntryError`] for more information.
    #[inline]
    pub fn get_palette_entry_mut(
        &mut self,
        index: [u8; 3],
    ) -> Result<&mut SSPaletteEntry<SD, T>, PaletteEntryError> {
        if self.is_deflated() {
            return Err(PaletteEntryError::DeflatedStorage);
        }

        let entry = self
            .get_entry(index)
            .ok_or(PaletteEntryError::OutOfBounds(index))?;

        if !entry.points_to_subdivided() {
            return Err(PaletteEntryError::NotSubdivided(entry.0, index));
        }

        let palette_index = entry.as_usize();
        Ok(&mut self.sdiv_palette[palette_index])
    }

    /// Get an entry of the storage's indices, returning [`None`] if the provided index is out of bounds.
    #[inline]
    pub fn get_entry(&self, index: [u8; 3]) -> Option<SSIndexEntry<T>> {
        let [i0, i1, i2] = index;
        self.indices
            .get(i0 as usize)?
            .get(i1 as usize)?
            .get(i2 as usize)
            .copied()
    }

    /// Get a mutable reference to an entry of the storage's indices,
    /// returning [`None`] if the provided index is out of bounds.
    #[inline]
    pub fn get_entry_mut(&mut self, index: [u8; 3]) -> Option<&mut SSIndexEntry<T>> {
        let [i0, i1, i2] = index;
        self.indices
            .get_mut(i0 as usize)?
            .get_mut(i1 as usize)?
            .get_mut(i2 as usize)
    }

    /// Get an index-level value from this storage, returning an error if the provided index is out of
    /// bounds or the entry at the index is subdivided. Will never query the palette for microblock values.
    /// Use [`Self::get_mb`] to get microblocks.
    #[inline(always)]
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
    #[inline(always)]
    pub fn set(&mut self, index: [u8; 3], value: T) -> Result<(), SubdivAccessError> {
        let entry = self
            .get_entry_mut(index)
            .ok_or(SubdivAccessError::OutOfBounds(index))?;

        *entry = SSIndexEntry::from_value(value);
        Ok(())
    }

    /// Set a value in this storage at the microblock level, returning an error if the index is out of bounds.
    /// Will automatically create and allocate a new palette entry if one doesn't already exist.
    /// ### Performance Warning
    /// Consider the following conditions:
    /// - The position that's being written to is part of an already subdivided block.
    /// - The storage is deflated.
    ///
    /// In this case, the storage *must* inflate for the write operation to work. This function will
    /// do that automatically, and it can be quite expensive and also likely makes the memory footprint
    /// of the storage bigger *(often by a lot!)*. Keep this in mind when writing and deflating. Try to
    /// batch together as many writes as you can *before* you deflate the storage.
    ///
    /// Subsequent writes after an inflationary call to this function will be just as fast as normal though.
    #[inline(always)]
    pub fn set_mb(&mut self, mb_index: [u8; 3], value: T) -> Result<(), SubdivAccessError> {
        let index = mb_index_to_index(mb_index, SD as u8);
        let new_index = self.sdiv_palette.len() as u32;
        let entry = self
            .get_entry_mut(index)
            .ok_or(SubdivAccessError::OutOfBounds(index))?;

        if entry.points_to_subdivided() {
            // We're trying to write a microblock, so we need to be in an inflated state.
            self.inflate();

            // New entry, this time pointing to a unique index in the palette.
            let entry = self
                .get_entry_mut(index)
                .ok_or(SubdivAccessError::OutOfBounds(index))?;

            let palette_index = entry.as_usize();
            let palette_entry = &mut self.sdiv_palette[palette_index];

            let local_index = mb_index_to_local_mb_index(mb_index, SD as u8);
            debug_assert!(local_index.iter().all(|c| *c < SD as u8));

            // SAFETY: 'mb_index_to_local_mb_index' can't return anything larger than 'SD'
            unsafe {
                palette_entry.set_unchecked(local_index, value);
            }

            // Reset the cached hash since the data is now changed.
            palette_entry.cached_hash = None;

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
        let mut new_indices = self.indices.clone();
        let mut new_palette = Vec::with_capacity(self.sdiv_palette.len());

        for i0 in 0..D {
            for i1 in 0..D {
                for i2 in 0..D {
                    let entry = &mut new_indices[i0][i1][i2];

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

    /// Inflate this storage's palette, making it take more space in memory but enabling faster
    /// writes *(see below)* to microblock values. Reading behaviour should be symmetrical between both inflated
    /// and deflated storages. If the storage is already inflated, this function is a no-op.
    ///
    /// ### Performance Warning
    /// Writing microblocks to a previously subdivided value is not actually possible unless the
    /// storage is inflated. Such writes will automatically inflate the storage, which can be slow
    /// and unexpected. See [`SubdividedStorage::set_mb`] for more information.
    #[inline]
    pub fn inflate(&mut self) {
        // Split up the different fields that interest us so that the borrowchecker doesn't get confused.
        let Self {
            indices,
            sdiv_palette,
            palette_kind,
            ..
        } = self;

        let SubdivPaletteKind::Deflated(inflated_size) = *palette_kind else {
            // Palette is already inflated, so there's nothing to do.
            return;
        };

        // If we inflate the palette then there's a decent chance that we'll end up inserting
        // a new subdivided value, so we don't use reserve_exact here.
        sdiv_palette.reserve(inflated_size);

        let palette_length = sdiv_palette.len();
        // Keeps track of which indices we've visited at least once before. If we naively insert
        // a new palette entry every time we encounter a subdivided value, we won't get to re-use the already
        // present values in the palette!
        type Buf = SmallVec<[bool; INFLATION_INSERTION_TRACKING_BUFFER_SIZE]>;
        let mut inserted: Buf = smallvec::smallvec![false; palette_length];

        for i0 in 0..D {
            for i1 in 0..D {
                for i2 in 0..D {
                    let entry: &mut SSIndexEntry<_> = &mut indices[i0][i1][i2];

                    // Skip values that are not subdivided, since they don't have to touch the palette.
                    if !entry.points_to_subdivided() {
                        continue;
                    }

                    let palette_index = entry.as_usize();
                    // Only insert if this is NOT the first time we encounter this index.
                    if inserted[palette_index] {
                        let subdivided = sdiv_palette[palette_index].clone();
                        let duplicate_index = sdiv_palette.len();
                        sdiv_palette.push(subdivided);

                        *entry = SSIndexEntry::from_index(duplicate_index as u32);
                    }

                    inserted[palette_index] = true;
                }
            }
        }

        *palette_kind = SubdivPaletteKind::Inflated;
    }

    /// Deflate this storage's palette, giving the storage a smaller memory footprint at the cost of
    /// slow writes. Reading performance should be the same in both deflated and inflated storages.
    /// You can provide an estimate of the amount of unique subdivided values in this storage.
    /// Choosing a good estimate can potentially speed up deflation a bit.
    ///
    /// ### Performance Warning
    /// Deflation is slow and inflating a previously deflated storage is slow. Due to the fact that
    /// inflation happens automatically when writing to a storage (see [docs]) you should only deflate
    /// when you're fairly sure that you won't be writing to this chunk again for a while.
    /// Also check the docs on [`SubdividedStorage::set_mb`] for some more information about this behaviour.
    ///
    /// [docs]: [`SubdividedStorage::inflate`]
    #[inline(never)]
    pub fn deflate(&mut self, unique: Option<usize>) {
        // Split up the different fields that interest us so that the borrow checker doesn't get confused.
        let Self {
            indices,
            sdiv_palette,
            palette_kind,
            ..
        } = self;

        // This may be called on a deflated storage to further deflate it, in which case we don't want to use
        // the already inflated palette's length.
        let old_palette_len = match palette_kind {
            SubdivPaletteKind::Inflated => sdiv_palette.len(),
            SubdivPaletteKind::Deflated(old_len) => *old_len,
        };

        let capacity = unique.unwrap_or(0);

        // Need to initialize the hashmap with the same random state we use for caching hashes
        let mut visited: HashMap<[[[T; SD]; SD]; SD], usize, ahash::RandomState> =
            HashMap::with_capacity_and_hasher(capacity, self.random_state.clone());

        let mut new_palette: Vec<SSPaletteEntry<SD, T>> =
            unique.map(Vec::with_capacity).unwrap_or_default();

        for i0 in 0..D {
            for i1 in 0..D {
                for i2 in 0..D {
                    let index_entry = &mut indices[i0][i1][i2];

                    // Skip values that are not subdivided, since they don't have to touch the palette.
                    if !index_entry.points_to_subdivided() {
                        continue;
                    }

                    let palette_index = index_entry.as_usize();

                    let palette_entry = &mut sdiv_palette[palette_index];

                    // Coalesce subdivided values into a whole block when possible.
                    if palette_entry.all_equal() {
                        *index_entry = SSIndexEntry::from_value(palette_entry.first());
                        continue;
                    }

                    let hash = palette_entry.cached_hash_compute(&self.random_state);

                    let visited_entry = visited
                        .raw_entry_mut()
                        .from_hash(hash, |data| &palette_entry.data == data);

                    visited_entry
                        .and_modify(|_, &mut index| {
                            // We've already come across this value before.
                            *index_entry = SSIndexEntry::from_index(index as u32);
                        })
                        .or_insert_with(|| {
                            // This is the first time we're seeing (and inserting) this value.

                            palette_entry.cached_hash = Some(hash);
                            let new_palette_index = new_palette.len();
                            *index_entry = SSIndexEntry::from_index(new_palette_index as u32);

                            new_palette.push(palette_entry.clone());

                            (palette_entry.data, new_palette_index)
                        });
                }
            }
        }

        *palette_kind = SubdivPaletteKind::Deflated(old_palette_len);
        *sdiv_palette = new_palette;
    }
}

pub const INFLATION_INSERTION_TRACKING_BUFFER_SIZE: usize = 16;

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

    #[test]
    fn deflate_and_inflate() {
        const BASE: [u8; 3] = [12, 60, 12];

        fn suite(s: &SubdividedStorage<16, 4, u32>) {
            for i0 in 0..64 {
                for i1 in 0..2 {
                    for i2 in 0..64 {
                        // The minimum corner block is not 1337
                        if [i0, i1, i2].iter().all(|&i| i < 4) {
                            continue;
                        }

                        let v = s.get_mb([i0, i1, i2]).unwrap();
                        assert_eq!(1337, v);
                    }
                }
            }

            for i0 in 0..4 {
                for i1 in 0..4 {
                    for i2 in 0..4 {
                        let index = [i0 + BASE[0], i1 + BASE[1], i2 + BASE[2]];

                        assert_eq!(404, s.get_mb(index).unwrap());
                    }
                }
            }

            assert_eq!(41, s.get([8, 8, 8]).unwrap());
            assert_eq!(42, s.get([8, 9, 8]).unwrap());
            assert_eq!(43, s.get([0, 0, 0]).unwrap());

            assert_eq!(101, s.get_mb([63, 63, 63]).unwrap());
            assert_eq!(100, s.get_mb([60, 60, 60]).unwrap());
        }

        let mut s = SubdividedStorage::<16, 4, u32>::new(0);

        for i0 in 0..64 {
            for i1 in 0..2 {
                for i2 in 0..64 {
                    s.set_mb([i0, i1, i2], 1337).unwrap();
                }
            }
        }

        s.set([8, 8, 8], 41).unwrap();
        s.set([8, 9, 8], 42).unwrap();
        s.set([0, 0, 0], 43).unwrap();

        s.set_mb([63, 63, 63], 101).unwrap();
        s.set_mb([60, 60, 60], 100).unwrap();

        for i0 in 0..4 {
            for i1 in 0..4 {
                for i2 in 0..4 {
                    let index = [i0 + BASE[0], i1 + BASE[1], i2 + BASE[2]];

                    s.set_mb(index, 404).unwrap();
                }
            }
        }

        suite(&s);

        s.deflate(None);
        assert_eq!(2, s.palette_len());

        suite(&s);

        s.inflate();

        suite(&s);
    }
}
