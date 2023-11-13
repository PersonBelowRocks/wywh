use std::array;

use bevy::math::IVec3;

use crate::{
    topo::chunk::Chunk,
    util::{self, CubicArray, SquareArray},
};

use super::error::OutOfBounds;

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

#[cfg(test)]
mod tests {
    use bevy::math::ivec3;

    use super::*;

    #[test]
    fn test_layered_chunk_storage_y() {
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
    fn test_layered_chunk_storage_xz() {
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
    fn test_layered_chunk_storage_raw_layer() {
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
}
