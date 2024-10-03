use std::{array, mem};

use glam::{ivec3, uvec3, IVec3, UVec3};
use hashbrown::{
    hash_map::{Entry, OccupiedEntry, VacantEntry},
    HashMap,
};
use num::Integer;
use rustc_hash::FxBuildHasher;
use slab::Slab;

use crate::{div_2_pow_n, rem_2_pow_n, urem_2_pow_n, Region};

fn empty_3d_array<const D: usize, T>() -> [[[Option<T>; D]; D]; D] {
    array::from_fn(|_| array::from_fn(|_| array::from_fn(|_| None)))
}

fn div_ivec_ceil(ivec: IVec3, n: i32) -> IVec3 {
    ivec3(
        Integer::div_ceil(&ivec.x, &n),
        Integer::div_ceil(&ivec.y, &n),
        Integer::div_ceil(&ivec.z, &n),
    )
}

fn div_ivec_floor(ivec: IVec3, n: i32) -> IVec3 {
    ivec3(
        Integer::div_floor(&ivec.x, &n),
        Integer::div_floor(&ivec.y, &n),
        Integer::div_floor(&ivec.z, &n),
    )
}

#[derive(Clone)]
pub struct Chunk<const D: usize, T> {
    pos: IVec3,
    data: [[[Option<T>; D]; D]; D],
    count: usize,
}

impl<const D: usize, T> Chunk<D, T> {
    /// Create a new empty chunk.
    ///
    /// # Panics
    /// Will panic if `D` is not a power of 2.
    #[inline]
    pub fn empty(pos: IVec3) -> Self {
        assert!(D.count_ones() == 1, "chunk dimensions must be a power of 2");

        Self {
            pos,
            data: empty_3d_array(),
            count: 0,
        }
    }

    /// Insert a value at the given position, returning the existing value if it existed.
    ///
    /// # Panics
    /// Will panic if the position is out of bounds.
    #[inline]
    pub fn insert(&mut self, p: UVec3, value: T) -> Option<T> {
        let [p0, p1, p2] = p.to_array().map(|n| n as usize);
        let old = mem::replace(&mut self.data[p0][p1][p2], Some(value));

        if old.is_none() {
            self.count += 1;
        }

        old
    }

    /// Remove a value from the given position, returning it if it existed.
    ///
    /// # Panics
    /// Will panic if the position is out of bounds.
    #[inline]
    pub fn remove(&mut self, p: UVec3) -> Option<T> {
        let [p0, p1, p2] = p.to_array().map(|n| n as usize);
        let old = mem::replace(&mut self.data[p0][p1][p2], None);

        if old.is_some() {
            self.count -= 1;
        }

        old
    }

    /// Remove all values in this chunk.
    #[inline]
    pub fn clear(&mut self) {
        self.data = empty_3d_array();
        self.count = 0;
    }

    /// The number of elements in this chunk. Will never exceed `D * D * D`.
    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    /// Returns `true` if there are no items in this chunk.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Returns `true` if the chunk contains a value at the given position.
    ///
    /// # Panics
    /// Will panic if the position is out of bounds.
    #[inline]
    pub fn contains(&self, p: UVec3) -> bool {
        let [p0, p1, p2] = p.to_array().map(|n| n as usize);
        self.data[p0][p1][p2].is_some()
    }

    /// Get a reference to the value at the given position, if it exists.
    ///
    /// # Panics
    /// Will panic if the position is out of bounds.
    #[inline]
    pub fn get(&self, p: UVec3) -> Option<&T> {
        let [p0, p1, p2] = p.to_array().map(|n| n as usize);
        self.data[p0][p1][p2].as_ref()
    }

    /// Get a mutable reference to the value at the given position, if it exists.
    ///
    /// # Panics
    /// Will panic if the position is out of bounds.
    #[inline]
    pub fn get_mut(&mut self, p: UVec3) -> Option<&mut T> {
        let [p0, p1, p2] = p.to_array().map(|n| n as usize);
        self.data[p0][p1][p2].as_mut()
    }

    /// Insert all values in the other chunk into this one.
    #[inline]
    pub fn append(&mut self, mut other: Self) {
        let max = D as u32;
        for (p0, p1, p2) in itertools::iproduct!(0..max, 0..max, 0..max) {
            let p = uvec3(p0, p1, p2);

            let Some(value) = other.remove(p) else {
                continue;
            };

            self.insert(p, value);
        }
    }
}

use entry::*;
pub mod entry {
    use super::*;

    pub struct VmOccupiedEntry<'a, const D: usize, T> {
        pub(super) hm_entry: OccupiedEntry<'a, IVec3, usize, FxBuildHasher>,
        pub(super) slab: &'a mut Slab<Chunk<D, T>>,
        pub(super) pos: UVec3,
    }

    impl<'a, const D: usize, T> VmOccupiedEntry<'a, D, T> {
        /// The chunk associated with this entry.
        #[inline]
        pub fn chunk(&self) -> &Chunk<D, T> {
            let chunk_index = *self.hm_entry.get();
            &self.slab[chunk_index]
        }

        /// Get a mutable reference to the chunk associated with this entry.
        #[inline]
        pub fn chunk_mut(&mut self) -> &mut Chunk<D, T> {
            let chunk_index = *self.hm_entry.get();
            &mut self.slab[chunk_index]
        }

        /// Get a reference to the value at this entry.
        #[inline]
        pub fn get(&self) -> &T {
            self.chunk().get(self.pos).unwrap()
        }

        /// Get a mutable reference to the value at this entry.
        #[inline]
        pub fn get_mut(&mut self) -> &mut T {
            // appease the borrowcker
            let pos = self.pos;
            self.chunk_mut().get_mut(pos).unwrap()
        }

        /// Convert the occupied entry into a mutable reference to the underlying value.
        #[inline]
        pub fn into_mut(self) -> &'a mut T {
            let chunk_index = *self.hm_entry.get();
            let [p0, p1, p2] = self.pos.to_array().map(|k| k as usize);
            self.slab[chunk_index].data[p0][p1][p2].as_mut().unwrap()
        }

        /// Remove the value at this entry.
        #[inline]
        pub fn remove(self) -> T {
            let [p0, p1, p2] = self.pos.to_array().map(|k| k as usize);
            let chunk_index = *self.hm_entry.get();
            let chunk = &mut self.slab[chunk_index];

            let old = chunk.data[p0][p1][p2].take().unwrap();
            chunk.count -= 1;

            if chunk.is_empty() {
                let chunk_index = self.hm_entry.remove();
                self.slab.remove(chunk_index);
            }

            old
        }

        /// Replace or remove the entry depending on which `Option<T>` variant the closure returns.
        #[inline]
        pub fn replace_entry_with<F>(mut self, f: F) -> VmEntry<'a, D, T>
        where
            F: FnOnce(T) -> Option<T>,
        {
            let [p0, p1, p2] = self.pos.to_array().map(|k| k as usize);
            let chunk = self.chunk_mut();
            let value = chunk.data[p0][p1][p2].take().unwrap();

            match f(value) {
                None => {
                    chunk.count -= 1;

                    // Remove the chunk if its empty
                    if chunk.is_empty() {
                        let chunk_index = *self.hm_entry.get();
                        self.slab.remove(chunk_index);

                        // This just creates an Entry::Vacant variant for us without having to do anything fancy.
                        let vacant = self.hm_entry.replace_entry_with(|_, _| None);

                        VmEntry::Vacant(VmVacantEntry {
                            hm_entry: vacant,
                            slab: self.slab,
                            pos: self.pos,
                        })
                    } else {
                        VmEntry::Vacant(VmVacantEntry {
                            hm_entry: Entry::Occupied(self.hm_entry),
                            slab: self.slab,
                            pos: self.pos,
                        })
                    }
                }
                Some(new_value) => {
                    chunk.data[p0][p1][p2] = Some(new_value);
                    VmEntry::Occupied(self)
                }
            }
        }
    }

    pub struct VmVacantEntry<'a, const D: usize, T> {
        pub(super) hm_entry: Entry<'a, IVec3, usize, FxBuildHasher>,
        pub(super) slab: &'a mut Slab<Chunk<D, T>>,
        pub(super) pos: UVec3,
    }

    impl<'a, const D: usize, T> VmVacantEntry<'a, D, T> {
        /// Get a reference to the chunk associated with this entry if it exists.
        #[inline]
        pub fn chunk(&self) -> Option<&Chunk<D, T>> {
            let Entry::Occupied(entry) = &self.hm_entry else {
                return None;
            };

            let chunk_index = *entry.get();
            Some(&self.slab[chunk_index])
        }

        /// Get a mutable reference to the chunk associated with this entry if it exists.
        #[inline]
        pub fn chunk_mut(&mut self) -> Option<&mut Chunk<D, T>> {
            let Entry::Occupied(entry) = &self.hm_entry else {
                return None;
            };

            let chunk_index = *entry.get();
            Some(&mut self.slab[chunk_index])
        }

        /// Insert a value into this entry.
        #[inline]
        pub fn insert(self, value: T) -> &'a mut T {
            match self.hm_entry {
                Entry::Occupied(entry) => {
                    let chunk_index = *entry.get();
                    let chunk = &mut self.slab[chunk_index];

                    chunk.insert(self.pos, value);
                    chunk.get_mut(self.pos).unwrap()
                }
                Entry::Vacant(entry) => {
                    let chunk_pos = *entry.key();
                    let mut chunk = Chunk::empty(chunk_pos);
                    chunk.insert(self.pos, value);

                    let chunk_index = self.slab.insert(chunk);
                    entry.insert(chunk_index);

                    self.slab[chunk_index].get_mut(self.pos).unwrap()
                }
            }
        }
    }

    pub enum VmEntry<'a, const D: usize, T> {
        Occupied(VmOccupiedEntry<'a, D, T>),
        Vacant(VmVacantEntry<'a, D, T>),
    }

    impl<'a, const D: usize, T> VmEntry<'a, D, T> {
        /// Equivalent to a hashmap entry's `and_modify` method.
        #[inline]
        pub fn and_modify<F>(mut self, f: F) -> Self
        where
            F: FnOnce(&mut T),
        {
            if let Self::Occupied(entry) = &mut self {
                f(entry.get_mut());
            }

            self
        }

        /// Equivalent to a hashmap entry's `or_insert` method.
        #[inline]
        pub fn or_insert(self, value: T) -> &'a mut T {
            match self {
                Self::Vacant(entry) => entry.insert(value),
                Self::Occupied(entry) => entry.into_mut(),
            }
        }

        /// Equivalent to a hashmap entry's `or_insert_with` method.
        #[inline]
        pub fn or_insert_with<F>(self, f: F) -> &'a mut T
        where
            F: FnOnce() -> T,
        {
            match self {
                Self::Vacant(entry) => entry.insert(f()),
                Self::Occupied(entry) => entry.into_mut(),
            }
        }

        /// Replace or remove an entry depending on the variant of the `Option<T>` returned by the closure.
        #[inline]
        pub fn and_replace_entry_with<F>(self, f: F) -> Self
        where
            F: FnOnce(T) -> Option<T>,
        {
            match self {
                Self::Vacant(_) => self,
                Self::Occupied(entry) => entry.replace_entry_with(f),
            }
        }
    }
}

/// A map-type data structure associating 3D integer positions with values.
#[derive(Clone)]
pub struct VoxelMap<T, const D: usize = 4> {
    slab: Slab<Chunk<D, T>>,
    chunks: HashMap<IVec3, usize, FxBuildHasher>,
}

impl<const D: usize, T> VoxelMap<T, D> {
    /// Split a position into a chunk position and a local position within that chunk.
    #[inline]
    fn chunk_and_local(p: IVec3) -> (IVec3, UVec3) {
        let chunk: IVec3 = p.to_array().map(|k| div_2_pow_n(k, D as u32)).into();
        let local: UVec3 = p.to_array().map(|k| rem_2_pow_n(k, D as u32) as u32).into();

        (chunk, local)
    }

    /// Create a new empty voxel map.
    pub fn new() -> Self {
        Self {
            slab: Slab::new(),
            chunks: HashMap::with_hasher(FxBuildHasher::default()),
        }
    }

    /// Create a new empty voxel map with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slab: Slab::with_capacity(capacity),
            chunks: HashMap::with_capacity_and_hasher(capacity, FxBuildHasher::default()),
        }
    }

    /// Insert a value at the given position, returning the old value if it exists.
    #[inline]
    pub fn insert(&mut self, p: IVec3, value: T) -> Option<T> {
        let (chunk_pos, local_pos) = Self::chunk_and_local(p);

        match self.chunks.entry(chunk_pos) {
            Entry::Occupied(entry) => {
                let chunk_index = *entry.get();
                let chunk = self.slab.get_mut(chunk_index).unwrap();
                chunk.insert(local_pos, value)
            }
            Entry::Vacant(entry) => {
                let mut chunk = Chunk::empty(chunk_pos);
                chunk.insert(local_pos, value);

                entry.insert(self.slab.insert(chunk));
                None
            }
        }
    }

    /// Remove a value from the voxel map and return it.
    #[inline]
    pub fn remove(&mut self, p: IVec3) -> Option<T> {
        let (chunk_pos, local_pos) = Self::chunk_and_local(p);

        let Entry::Occupied(entry) = self.chunks.entry(chunk_pos) else {
            return None;
        };

        let chunk_index = *entry.get();
        let chunk = self.slab.get_mut(chunk_index).unwrap();
        let old = chunk.remove(local_pos);

        if chunk.is_empty() {
            self.slab.remove(chunk_index);
            entry.remove();
        }

        old
    }

    #[inline]
    pub fn remove_region(&mut self, region: Region) {
        let chunk_min = div_ivec_ceil(region.min(), D as _);
        let chunk_max = div_ivec_floor(region.max(), D as _);

        let inner_chunk_region = Region::new(chunk_min, chunk_max);

        for (x, y, z) in itertools::iproduct!(
            chunk_min.x..=chunk_max.x,
            chunk_min.y..=chunk_max.y,
            chunk_min.z..=chunk_max.z
        ) {
            let chunk_pos = ivec3(x, y, z);
            let Some(chunk_index) = self.chunks.remove(&chunk_pos) else {
                continue;
            };

            self.slab.remove(chunk_index);
        }

        let outer_chunk_region = Region::new(
            div_ivec_floor(region.min(), D as _),
            div_ivec_ceil(region.max(), D as _),
        );

        for outer_chunk in outer_chunk_region
            .iter()
            .filter(|&p| !inner_chunk_region.contains(p))
        {}

        todo!()
    }

    /// Get a reference to a value in the voxel map.
    #[inline]
    pub fn get(&self, p: IVec3) -> Option<&T> {
        let (chunk_pos, local_pos) = Self::chunk_and_local(p);

        let Some(&chunk_index) = self.chunks.get(&chunk_pos) else {
            return None;
        };

        self.slab[chunk_index].get(local_pos)
    }

    /// Get a mutable reference to a value in the voxel map.
    #[inline]
    pub fn get_mut(&mut self, p: IVec3) -> Option<&mut T> {
        let (chunk_pos, local_pos) = Self::chunk_and_local(p);

        let Some(&chunk_index) = self.chunks.get(&chunk_pos) else {
            return None;
        };

        self.slab[chunk_index].get_mut(local_pos)
    }

    /// Returns `true` if this map has a value at the given position.
    #[inline]
    #[must_use]
    pub fn contains(&self, p: IVec3) -> bool {
        self.get(p).is_some()
    }

    /// Hashmap-like entry API but for the voxel map.
    #[inline]
    pub fn entry(&mut self, p: IVec3) -> VmEntry<'_, D, T> {
        let (chunk_pos, local_pos) = Self::chunk_and_local(p);

        if self.contains(p) {
            let Entry::Occupied(entry) = self.chunks.entry(chunk_pos) else {
                unreachable!("we just tested that we contained a value here, which means there must be a chunk at this position");
            };

            VmEntry::Occupied(VmOccupiedEntry {
                pos: local_pos,
                slab: &mut self.slab,
                hm_entry: entry,
            })
        } else {
            VmEntry::Vacant(VmVacantEntry {
                hm_entry: self.chunks.entry(chunk_pos),
                slab: &mut self.slab,
                pos: local_pos,
            })
        }
    }

    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.chunks.shrink_to_fit();
        self.slab.compact(|chunk, _, new_index| {
            self.chunks
                .entry(chunk.pos)
                .and_modify(|index| *index = new_index);
            true
        });
    }
}
