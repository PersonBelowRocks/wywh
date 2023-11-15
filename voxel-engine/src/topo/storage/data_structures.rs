use std::{array, mem, ptr};

use bevy::math::{ivec3, IVec3};

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
/// An ICS is similar to the `indexmap` crate, it stores the actual data in a vector and holds
/// a separate `DenseChunkStorage` full of indices into that vector. This is useful when storing data that is duplicate
/// across many positions, such as various voxel attributes and whatnot.
///
/// The tradeoff here is that an ICS must be optimized between insertions to get rid of duplicates. This isn't necessary for it
/// to actually function but it won't automatically handle duplicates otherwise and therefore will use more memory than needed,
/// thereby defeating one of the main points of actually using an ICS in the first place (note that the [`IndexedChunkStorage::set_many`]
/// method only partially handles duplicates, see the method documentation for more info).
/// Optimizing is done by calling the `optimize` method, but it's quite slow so it should be done sparingly.
#[derive(Clone)]
pub struct IndexedChunkStorage<T: PartialEq> {
    indices: DenseChunkStorage<u16>,
    values: Vec<T>,
    last_index: usize,
}

impl<T: PartialEq> IndexedChunkStorage<T> {
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
        self.last_index = idx;
    }

    pub fn new() -> Self {
        Self {
            indices: DenseChunkStorage::new(Self::EMPTY_VALUE),
            values: Vec::new(),
            last_index: 0,
        }
    }

    pub fn contains_pos(pos: IVec3) -> bool {
        Chunk::BOUNDING_BOX.contains(pos)
    }

    pub fn contains_value(&self, value: &T) -> bool {
        self.values.contains(value)
    }

    /// Set the given `pos` to `data`. If the value at `pos` is equal to `data` then the function will return an [`Ok(Some(T))`] containing the data, giving it back.
    /// Otherwise returns [`Ok(None)`] on success or [`Err(OutOfBounds)`] if the `pos` is out of bounds.
    pub fn set(&mut self, pos: IVec3, data: T) -> Result<Option<T>, OutOfBounds> {
        if !Self::contains_pos(pos) {
            return Err(OutOfBounds);
        }

        if self.values.get(self.last_index) == Some(&data) {
            self.set_idx(pos, self.last_index);
        }

        match self.get_idx(pos) {
            Some(idx) => {
                if self.values[idx] != data {
                    self.set_idx(pos, self.values.len());
                    self.values.push(data);
                    return Ok(None);
                }
            }
            None => {
                self.set_idx(pos, self.values.len());
                self.values.push(data);
                return Ok(None);
            }
        }

        Ok(Some(data))
    }

    pub fn clear(&mut self, pos: IVec3) -> Result<(), OutOfBounds> {
        if !Self::contains_pos(pos) {
            return Err(OutOfBounds);
        }

        let upos = util::try_ivec3_to_usize_arr(pos).unwrap();
        *self.indices.get_mut(upos).unwrap() = Self::EMPTY_VALUE;

        Ok(())
    }

    /// Sets all the given `positions` to `data`, only using a single copy of `data`.
    /// Does not check for existing copies of `data` to index to.
    /// Returns [`Err(OutOfBounds)`] if all `positions` are out of bounds.
    pub fn set_many(&mut self, positions: &[IVec3], data: T) -> Result<(), OutOfBounds> {
        let idx = self.values.len();
        let mut all_oob = true;

        for &pos in positions.iter() {
            if !Self::contains_pos(pos) {
                continue;
            }

            all_oob = false;
            self.set_idx(pos, idx);
        }

        if !all_oob {
            self.values.push(data);
            Ok(())
        } else {
            Err(OutOfBounds)
        }
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

    pub fn values(&self) -> usize {
        self.values.len()
    }

    pub fn optimize(&mut self) -> usize {
        let old_value_count = self.values();

        let new_values = Vec::<T>::with_capacity(self.values());
        let new_indices = DenseChunkStorage::<u16>::new(Self::EMPTY_VALUE);

        let old_values = mem::replace(&mut self.values, new_values);
        let old_indices = mem::replace(&mut self.indices, new_indices);

        let mut moved_indices = hb::HashMap::<usize, usize>::with_capacity(self.values());

        for x in 0..Chunk::SIZE {
            for y in 0..Chunk::SIZE {
                for z in 0..Chunk::SIZE {
                    let pos = ivec3(x, y, z);

                    let old_index = old_indices.get(pos).unwrap();
                    if old_index == Self::EMPTY_VALUE {
                        continue;
                    }

                    if let Some(&new_index) = moved_indices.get(&(old_index as usize)) {
                        self.set_idx(pos, new_index);
                        continue;
                    }

                    let data = &old_values[old_index as usize];
                    match self.values.iter().enumerate().find(|(_, v)| v == &data) {
                        Some((existing_index, _)) => {
                            self.set_idx(pos, existing_index);
                        }
                        None => {
                            // TODO: get miri to take a little look at this
                            // SAFETY: this address is valid to read from (came from a regular borrow/reference)
                            // and we keep track of everything we've read from so we don't read the same address twice
                            // (so we're basically moving the value here)
                            let data =
                                unsafe { ptr::read(&old_values[old_index as usize] as *const T) };

                            let new_index = self.values.len();
                            self.values.push(data);
                            moved_indices.insert(old_index as usize, new_index);
                        }
                    }
                }
            }
        }

        self.values.shrink_to_fit();
        let new_value_count = self.values();

        old_value_count - new_value_count
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

        assert_eq!(2, ics.values());

        ics.clear(ivec3(0, 0, 0)).unwrap();

        assert_eq!(None, ics.get(ivec3(0, 0, 0)).unwrap());

        assert_eq!(2, ics.values());
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

        // this guy is different!
        ics.set(ivec3(0, 2, 0), 11).unwrap();

        ics.set(ivec3(0, 1, 0), 10).unwrap();

        assert_eq!(3, ics.values());

        assert_eq!(1, ics.optimize());

        assert_eq!(2, ics.values());
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

        assert_eq!(1, ics.values());
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

        assert_eq!(1, ics.values());
    }
}
