use std::{
    array,
    hash::{self, BuildHasher},
};

use bevy::math::{ivec3, IVec3};
use hashbrown::HashTable;

use crate::{
    topo::{access::ReadAccess, chunk::Chunk},
    util::{self, CubicArray, SquareArray},
};

use super::error::OutOfBounds;

// TODO: octree based storage

/// DCS for short
#[derive(Clone)]
pub struct DenseChunkStorage<T>(pub(crate) CubicArray<{ Chunk::USIZE }, T>);

impl<T: Copy> DenseChunkStorage<T> {
    pub fn new(filling: T) -> Self {
        Self([[[filling; Chunk::USIZE]; Chunk::USIZE]; Chunk::USIZE])
    }
}

impl<T> DenseChunkStorage<T> {
    pub fn get_ref(&self, idx: [usize; 3]) -> Option<&T> {
        let [x, y, z] = idx;
        self.0.get(x)?.get(y)?.get(z)
    }

    pub fn get_mut(&mut self, idx: [usize; 3]) -> Option<&mut T> {
        let [x, y, z] = idx;
        self.0.get_mut(x)?.get_mut(y)?.get_mut(z)
    }
}

type SqChunkArray<T> = SquareArray<{ Chunk::USIZE }, Option<T>>;

// TODO: tests & benchmarks
/// LCS for short
#[derive(Clone)]
pub struct LayeredChunkStorage<T: Sized>([Option<Box<SqChunkArray<T>>>; Chunk::USIZE]);

impl<T> LayeredChunkStorage<T> {
    pub fn new() -> Self {
        Self(array::from_fn(|_| None))
    }

    pub fn get_layer_mut(
        &mut self,
        idx: usize,
    ) -> Result<&mut Option<Box<SqChunkArray<T>>>, OutOfBounds> {
        self.0.get_mut(idx).ok_or(OutOfBounds)
    }

    pub fn get_layer(&self, idx: usize) -> Result<&Option<Box<SqChunkArray<T>>>, OutOfBounds> {
        self.0.get(idx).ok_or(OutOfBounds)
    }

    pub fn insert_layer(&mut self, data: SqChunkArray<T>, idx: usize) -> Result<(), OutOfBounds> {
        *self.get_layer_mut(idx)? = Some(Box::new(data));
        Ok(())
    }

    pub fn clear_layer(&mut self, idx: usize) -> Result<(), OutOfBounds> {
        *self.get_layer_mut(idx)? = None;
        Ok(())
    }

    pub fn clear_empty_layers(&mut self) -> usize {
        let mut cleared = 0;

        for y in 0..Chunk::USIZE {
            let mut should_clear = false;
            if let Some(layer) = self.get_layer(y).unwrap().as_deref() {
                should_clear = true;
                'outer: for x in 0..Chunk::USIZE {
                    for z in 0..Chunk::USIZE {
                        if layer[x][z].is_some() {
                            should_clear = false;
                            break 'outer;
                        }
                    }
                }
            }

            if should_clear {
                cleared += 1;
                self.clear_layer(y).unwrap()
            }
        }

        cleared
    }

    pub fn contains(pos: IVec3) -> bool {
        Chunk::BOUNDING_BOX.contains(pos)
    }

    pub fn get(&self, pos: IVec3) -> Result<Option<T>, OutOfBounds>
    where
        T: Copy,
    {
        let y = pos.y as usize;

        let layer = self.get_layer(y)?;
        let [x, _, z] = util::try_ivec3_to_usize_arr(pos).unwrap();
        if !Self::contains(pos) {
            return Err(OutOfBounds);
        }

        Ok(layer.as_ref().and_then(|l| l[x][z]))
    }

    pub fn set(&mut self, pos: IVec3, data: T) -> Result<(), OutOfBounds> {
        let y = pos.y as usize;

        let layer = self.get_layer_mut(y)?;
        let [x, _, z] = util::try_ivec3_to_usize_arr(pos).unwrap();
        if !Self::contains(pos) {
            return Err(OutOfBounds);
        }

        match layer {
            Some(inner) => {
                inner[x][z] = Some(data);
            }
            None => {
                let mut new_layer = SqChunkArray::<T>::default();
                new_layer[x][z] = Some(data);
                self.insert_layer(new_layer, y).unwrap();
            }
        }

        Ok(())
    }

    pub fn clear(&mut self, pos: IVec3) -> Result<(), OutOfBounds> {
        let y = pos.y as usize;

        let layer = self.get_layer_mut(y)?;
        let [x, _, z] = util::try_ivec3_to_usize_arr(pos).unwrap();
        if !Self::contains(pos) {
            return Err(OutOfBounds);
        }

        layer.as_deref_mut().map(|inner| inner[x][z] = None);

        Ok(())
    }
}

// TODO: is this applicable here? http://www.beosil.com/download/CollisionDetectionHashing_VMV03.pdf
/// HMCS/HMS for short
#[derive(Clone, Default)]
pub struct HashmapChunkStorage<T>(hb::HashMap<IVec3, T>);

impl<T> HashmapChunkStorage<T> {
    pub fn new() -> Self {
        Self(hb::HashMap::new())
    }

    pub fn contains(pos: IVec3) -> bool {
        Chunk::BOUNDING_BOX.contains(pos)
    }

    pub fn set(&mut self, pos: IVec3, data: T) -> Result<(), OutOfBounds> {
        if !Self::contains(pos) {
            return Err(OutOfBounds);
        }

        self.0.insert(pos, data);
        Ok(())
    }

    pub fn get(&self, pos: IVec3) -> Option<T>
    where
        T: Copy,
    {
        self.0.get(&pos).copied()
    }

    pub fn clear(&mut self, pos: IVec3) {
        self.0.remove(&pos);
    }
}

/// ICS for short.
///
/// An ICS is useful for storing duplicated voxel data. Internally it has a DCS, vector of all stored data, and a hash table mapping data to their indices.
/// Both reads and writes are in O(1) complexity, but writes are a bit slower than reads because they require a lookup in the hash table.
///
/// A flowchart for inserting a value where
///     `pos`: The position we're gonna insert to,
///     `val`: The value we're inserting,
///     `DCS`: [`DenseChunkStorage`] mapping positions to indices in V,
///     `V`: [`Vec`] of all our stored data,
///     `HT`: Hashtable that maps data to indices in V.
///
/// ```txt
/// if HT contains an index for val:
///     index = HT(val)
///     old_index = DCS(pos)
///     old_index = index
/// if HT does not contain val:
///     index = len(V)
///     HT.insert(val, index) (HT does not own val, it only stores val's hash. V owns val)
///     V.push(val)
///     old_index = DCS(pos)
///     old_index = index
/// ```
///
/// Flowchart for reading a value, terms are same as for inserting (see above):
/// ```txt
/// Option<index> = DCS(pos)
/// if index is not None:
///     return V(index)
/// else:
///     return None
/// ```
///
/// Removal of data (by calling `self.clear(pos)`) at a position is done by just removing the index at that position.
/// It does NOT remove the actual underlying data. It just makes it impossible to access.
/// The reason for this is because there is no way to get all positions that point to a piece of data without doing
/// an expensive linear search through the whole storage (`O(chunk volume)`) and then a bunch of shuffling of data to keep the vector
/// of values contiguous and also remapping every position to point to the new, shuffled, indices.
///
/// Doing all this every time you write to the storage, quite frankly, sucks, and is a terrible idea that makes it (almost) unusable.
/// Therefore there is a provided "garbage collection" method: [`Self::optimize`]. This method does all the reshuffling and remapping mentioned above
/// and is designed to do so as quickly as it can. It's good to optimize your ICS if you've done a lot of removals or overwrites of values to keep the memory
/// footprint down.
///
/// ICSes are an incredible tool when you need super fast reads, but writes aren't as important.
/// Or when you need to store lots of duplicate data (like various voxel attributes) without using
/// a bunch of redudant memory.
#[derive(Clone)]
pub struct IndexedChunkStorage<T: Eq + hash::Hash, S: BuildHasher = ahash::RandomState> {
    indices: DenseChunkStorage<u16>,
    values: Vec<T>,
    idx_table: HashTable<usize>,
    random_state: S,
}

fn optimize_by_copying<T: Eq + hash::Hash + Clone, S: BuildHasher + Clone>(
    old: &IndexedChunkStorage<T, S>,
) -> IndexedChunkStorage<T, S> {
    let mut new = IndexedChunkStorage::with_random_state(old.random_state.clone());

    for x in 0..Chunk::SIZE {
        for y in 0..Chunk::SIZE {
            for z in 0..Chunk::SIZE {
                let pos = ivec3(x, y, z);
                if let Some(vxl) = old.get(pos).unwrap() {
                    new.set(pos, vxl.clone()).unwrap();
                }
            }
        }
    }

    new.values.shrink_to_fit();
    new.idx_table
        .shrink_to_fit(|i| new.random_state.hash_one(&new.values[*i]));

    new
}

impl<T: Eq + hash::Hash> IndexedChunkStorage<T, ahash::RandomState> {
    pub fn new() -> Self {
        Self::internal_new(ahash::RandomState::new())
    }

    pub fn filled(value: T) -> Self {
        Self::filled_with_random_state(value, ahash::RandomState::new())
    }
}

impl<T: Eq + hash::Hash, S: BuildHasher> IndexedChunkStorage<T, S> {
    const EMPTY_VALUE: u16 = 0b1000_0000_0000_0000;

    fn get_idx(&self, pos: IVec3) -> Option<usize> {
        let us = util::try_ivec3_to_usize_arr(pos).unwrap();
        let idx = *self.indices.get_ref(us).unwrap();

        if idx != Self::EMPTY_VALUE {
            Some(idx as usize)
        } else {
            None
        }
    }

    fn set_idx(&mut self, pos: IVec3, idx: usize) {
        let us = util::try_ivec3_to_usize_arr(pos).unwrap();
        let slot = self.indices.get_mut(us).unwrap();
        *slot = idx as u16;
    }

    fn insert_new_unique_value(&mut self, pos: IVec3, data: T) {
        let idx = self.values.len();
        let hash = self.random_state.hash_one(&data);

        self.set_idx(pos, idx);
        self.values.push(data);

        let hasher = |i: &_| self.random_state.hash_one(&self.values[*i]);

        // this little allocation dance here seems to only improve performance on some systems, no clue whats going on
        // TODO: investigate!
        if self.idx_table.capacity() <= self.idx_table.len()
            && self.idx_table.capacity() < Chunk::USIZE.pow(3)
        {
            let max_add = Chunk::USIZE
                .pow(3)
                .saturating_sub(self.idx_table.capacity());
            let grow_by = usize::min(max_add, self.idx_table.len());

            self.idx_table.reserve(grow_by, hasher);
        }

        self.idx_table.insert_unique(hash, idx, hasher);
    }

    fn get_existing_index_for_data(&self, data: &T) -> Option<usize> {
        let hash = self.random_state.hash_one(data);
        self.idx_table
            .find(hash, |&i| &self.values[i] == data)
            .copied()
    }

    fn internal_new(random_state: S) -> Self {
        Self {
            indices: DenseChunkStorage::new(Self::EMPTY_VALUE),
            values: Vec::new(),
            idx_table: HashTable::new(),
            random_state,
        }
    }

    pub fn with_random_state(random_state: S) -> Self {
        Self::internal_new(random_state)
    }

    pub fn filled_with_random_state(filling: T, random_state: S) -> Self {
        let mut new = Self::internal_new(random_state);

        // all indices point to the first value in the value vector
        new.indices = DenseChunkStorage::new(0);
        new.values = vec![filling];

        new
    }

    pub fn values(&self) -> &[T] {
        &self.values
    }

    pub fn values_mut(&mut self) -> &mut [T] {
        &mut self.values
    }

    pub fn contains_pos(pos: IVec3) -> bool {
        Chunk::BOUNDING_BOX.contains(pos)
    }

    pub fn contains_value(&self, value: &T) -> bool {
        self.values.contains(value)
    }

    pub fn set(&mut self, pos: IVec3, data: T) -> Result<Option<T>, OutOfBounds> {
        if !Self::contains_pos(pos) {
            return Err(OutOfBounds);
        }

        match self.get_existing_index_for_data(&data) {
            Some(existing_index) => {
                self.set_idx(pos, existing_index);
                return Ok(Some(data));
            }
            None => self.insert_new_unique_value(pos, data),
        }

        Ok(None)
    }

    pub fn clear(&mut self, pos: IVec3) -> Result<(), OutOfBounds> {
        if !Self::contains_pos(pos) {
            return Err(OutOfBounds);
        }

        let upos = util::try_ivec3_to_usize_arr(pos).unwrap();
        *self.indices.get_mut(upos).unwrap() = Self::EMPTY_VALUE;

        Ok(())
    }

    pub fn set_many(&mut self, positions: &[IVec3], data: T) -> Result<(), OutOfBounds> {
        if positions.iter().any(|&pos| !Self::contains_pos(pos)) {
            return Err(OutOfBounds);
        }

        let idx = self.get_existing_index_for_data(&data).unwrap_or_else(|| {
            let idx = self.values.len();
            let hash = self.random_state.hash_one(&data);

            self.values.push(data);

            let hasher = |i: &_| self.random_state.hash_one(&self.values[*i]);
            self.idx_table.insert_unique(hash, idx, hasher);

            idx
        });

        for &position in positions {
            self.set_idx(position, idx)
        }

        Ok(())
    }

    pub fn get(&self, pos: IVec3) -> Result<Option<&T>, OutOfBounds> {
        if !Self::contains_pos(pos) {
            return Err(OutOfBounds);
        }

        let Some(idx) = self.get_idx(pos) else {
            return Ok(None);
        };

        Ok(Some(&self.values[idx]))
    }

    pub fn get_mut(&mut self, pos: IVec3) -> Result<Option<&mut T>, OutOfBounds> {
        if !Self::contains_pos(pos) {
            return Err(OutOfBounds);
        }

        let Some(idx) = self.get_idx(pos) else {
            return Ok(None);
        };

        Ok(Some(&mut self.values[idx]))
    }

    pub fn values_len(&self) -> usize {
        self.values().len()
    }
}

impl<T: hash::Hash + Eq + Clone, S: BuildHasher + Clone> IndexedChunkStorage<T, S> {
    pub fn optimize(&mut self) -> usize {
        let old_values = self.values_len();

        let new = optimize_by_copying(self);
        let new_values = new.values_len();

        *self = new;
        old_values - new_values
    }
}

#[allow(non_snake_case)]
#[cfg(test)]
mod tests {
    use bevy::math::ivec3;

    use super::*;

    #[test]
    fn test_LCS_y() {
        let mut storage = LayeredChunkStorage::<u32>::new();

        assert!(storage.set(ivec3(0, 0, 0), 10).is_ok());
        assert!(storage.set(ivec3(0, 1, 0), 11).is_ok());
        assert!(storage.set(ivec3(0, 2, 0), 12).is_ok());
        assert!(storage.set(ivec3(0, 3, 0), 13).is_ok());

        assert!(storage.set(ivec3(0, 15, 0), 14).is_ok());

        assert!(storage.set(ivec3(0, 16, 0), 99).is_err());
        assert!(storage.set(ivec3(0, -1, 0), 99).is_err());

        assert_eq!(Some(10), storage.get(ivec3(0, 0, 0)).unwrap());
        assert_eq!(Some(11), storage.get(ivec3(0, 1, 0)).unwrap());
        assert_eq!(Some(12), storage.get(ivec3(0, 2, 0)).unwrap());
        assert_eq!(Some(13), storage.get(ivec3(0, 3, 0)).unwrap());

        assert_eq!(Some(14), storage.get(ivec3(0, 15, 0)).unwrap());

        assert!(storage.get(ivec3(0, 16, 0)).is_err());
        assert!(storage.get(ivec3(0, -1, 0)).is_err());
    }

    #[test]
    fn test_LCS_xz() {
        let mut storage = LayeredChunkStorage::<u32>::new();

        assert!(storage.set(ivec3(15, 0, 15), 10).is_ok());
        assert!(storage.set(ivec3(10, 0, 10), 11).is_ok());

        assert!(storage.set(ivec3(16, 0, 15), 99).is_err());
        assert!(storage.set(ivec3(15, 0, 16), 99).is_err());

        for x in 0..Chunk::SIZE {
            for z in 0..Chunk::SIZE {
                assert!(storage.set(ivec3(x, 4, z), 50).is_ok())
            }
        }

        for x in 0..Chunk::SIZE {
            for z in 0..Chunk::SIZE {
                assert_eq!(Some(50), storage.get(ivec3(x, 4, z)).unwrap())
            }
        }

        assert_eq!(Some(10), storage.get(ivec3(15, 0, 15)).unwrap());
        assert_eq!(Some(11), storage.get(ivec3(10, 0, 10)).unwrap());
    }

    #[test]
    fn test_LCS_raw_layer() {
        let mut storage = LayeredChunkStorage::<u32>::new();

        let mut arr = <[[Option<u32>; Chunk::USIZE]; Chunk::USIZE]>::default();
        arr[14][8] = Some(10);
        arr[15][15] = Some(11);
        arr[0][0] = Some(12);

        storage.insert_layer(arr, 8).unwrap();

        assert!(storage.get_layer(8).unwrap().is_some());

        assert_eq!(Some(10), storage.get(ivec3(14, 8, 8)).unwrap());
        assert_eq!(Some(11), storage.get(ivec3(15, 8, 15)).unwrap());
        assert_eq!(Some(12), storage.get(ivec3(0, 8, 0)).unwrap());

        assert!(storage.clear(ivec3(14, 8, 8)).is_ok());
        assert!(storage.clear(ivec3(15, 8, 15)).is_ok());
        assert!(storage.clear(ivec3(0, 8, 0)).is_ok());

        assert!(storage.get_layer(8).unwrap().is_some());

        assert_eq!(1, storage.clear_empty_layers());

        assert!(storage.get_layer(8).unwrap().is_none());
    }

    // TODO: we're gonna need more ICS tests!!!!!
    #[test]
    fn test_ICS_basics() {
        let mut ics = IndexedChunkStorage::<u32>::new();

        ics.set(ivec3(0, 0, 0), 10).unwrap();
        ics.set(ivec3(15, 15, 15), 11).unwrap();

        assert!(ics.set(ivec3(15, 16, 15), 12).is_err());

        assert_eq!(Some(&10), ics.get(ivec3(0, 0, 0)).unwrap());
        assert_eq!(None, ics.get(ivec3(0, 1, 0)).unwrap());
        assert_eq!(Some(&11), ics.get(ivec3(15, 15, 15)).unwrap());

        assert_eq!(2, ics.values_len());

        ics.clear(ivec3(0, 0, 0)).unwrap();

        assert_eq!(None, ics.get(ivec3(0, 0, 0)).unwrap());

        assert_eq!(2, ics.values_len());
    }

    #[test]
    fn test_ICS_optimizing() {
        let mut ics = IndexedChunkStorage::<u32>::new();

        ics.set_many(
            &[
                ivec3(0, 4, 0),
                ivec3(1, 4, 0),
                ivec3(2, 4, 0),
                ivec3(3, 4, 0),
            ],
            10,
        )
        .unwrap();

        ics.set(ivec3(0, 2, 0), 11).unwrap();

        ics.set(ivec3(0, 1, 0), 10).unwrap();

        assert_eq!(2, ics.values_len());

        // we clear out only '11' in the storage
        ics.clear(ivec3(0, 2, 0)).unwrap();

        // upon optimizing it should be removed
        assert_eq!(1, ics.optimize());

        // we should only have a '10' left in here
        assert_eq!(1, ics.values_len());
    }

    #[test]
    fn test_ICS_set_many() {
        let mut ics = IndexedChunkStorage::<u32>::new();

        ics.set_many(
            &[
                ivec3(0, 4, 0),
                ivec3(1, 4, 0),
                ivec3(2, 4, 0),
                ivec3(3, 4, 0),
            ],
            10,
        )
        .unwrap();

        assert_eq!(1, ics.values_len());
        assert_eq!(Some(&10), ics.get(ivec3(0, 4, 0)).unwrap());
        assert_eq!(Some(&10), ics.get(ivec3(1, 4, 0)).unwrap());
        assert_eq!(Some(&10), ics.get(ivec3(2, 4, 0)).unwrap());
        assert_eq!(Some(&10), ics.get(ivec3(3, 4, 0)).unwrap());

        assert!(ics
            .set_many(
                &[
                    ivec3(0, 4, 16),
                    ivec3(1, 4, 16),
                    ivec3(2, 4, 16),
                    ivec3(3, 4, 16),
                ],
                11
            )
            .is_err());

        assert_eq!(1, ics.values_len());
    }

    #[test]
    fn test_ICS_overwrite() {
        let mut ics = IndexedChunkStorage::<u32>::new();

        ics.set(ivec3(2, 2, 2), 10).unwrap();

        ics.set(ivec3(2, 3, 2), 11).unwrap();
        ics.set(ivec3(2, 3, 3), 11).unwrap();
        ics.set(ivec3(3, 2, 2), 11).unwrap();

        // overwrite our first value
        ics.set(ivec3(2, 2, 2), 11).unwrap();

        assert_eq!(2, ics.values_len());

        assert_eq!(1, ics.optimize());

        assert_eq!(1, ics.values_len());

        assert_eq!(Some(&11), ics.get(ivec3(2, 2, 2)).unwrap());
        assert_eq!(Some(&11), ics.get(ivec3(2, 3, 2)).unwrap());
        assert_eq!(Some(&11), ics.get(ivec3(2, 3, 3)).unwrap());
        assert_eq!(Some(&11), ics.get(ivec3(3, 2, 2)).unwrap());
        assert_eq!(Some(&11), ics.get(ivec3(2, 2, 2)).unwrap());
    }

    #[test]
    fn test_ICS_clear() {
        let mut ics = IndexedChunkStorage::<u32>::new();

        ics.set(ivec3(0, 0, 0), 10).unwrap();

        ics.set(ivec3(1, 0, 0), 11).unwrap();
        ics.set(ivec3(2, 0, 0), 11).unwrap();
        ics.set(ivec3(3, 0, 0), 11).unwrap();

        ics.clear(ivec3(0, 0, 0)).unwrap();

        assert_eq!(2, ics.values_len());

        assert_eq!(None, ics.get(ivec3(0, 0, 0)).unwrap());

        assert_eq!(1, ics.optimize());

        assert_eq!(1, ics.values_len());

        assert_eq!(None, ics.get(ivec3(0, 0, 0)).unwrap());

        assert_eq!(Some(&11), ics.get(ivec3(1, 0, 0)).unwrap());
        assert_eq!(Some(&11), ics.get(ivec3(2, 0, 0)).unwrap());
        assert_eq!(Some(&11), ics.get(ivec3(3, 0, 0)).unwrap());
    }

    #[test]
    fn test_ICS_reordering_optimization() {
        let mut ics = IndexedChunkStorage::<u32>::new();

        ics.set(ivec3(0, 0, 0), 10).unwrap();
        ics.set(ivec3(0, 1, 0), 11).unwrap();
        ics.set(ivec3(0, 2, 0), 12).unwrap();
        ics.set(ivec3(0, 3, 0), 13).unwrap();

        ics.clear(ivec3(0, 1, 0)).unwrap();
        ics.clear(ivec3(0, 2, 0)).unwrap();

        assert_eq!(Some(&10), ics.get(ivec3(0, 0, 0)).unwrap());
        assert_eq!(Some(&13), ics.get(ivec3(0, 3, 0)).unwrap());

        assert_eq!(None, ics.get(ivec3(0, 1, 0)).unwrap());
        assert_eq!(None, ics.get(ivec3(0, 2, 0)).unwrap());

        assert_eq!(4, ics.values_len());

        assert_eq!(2, ics.optimize());

        assert_eq!(2, ics.values_len());

        assert_eq!(Some(&10), ics.get(ivec3(0, 0, 0)).unwrap());
        assert_eq!(Some(&13), ics.get(ivec3(0, 3, 0)).unwrap());

        assert_eq!(None, ics.get(ivec3(0, 1, 0)).unwrap());
        assert_eq!(None, ics.get(ivec3(0, 2, 0)).unwrap());
    }
}
