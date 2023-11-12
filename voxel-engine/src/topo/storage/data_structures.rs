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

    pub(crate) fn get_layer_mut(
        &mut self,
        idx: usize,
    ) -> Result<&mut Option<Box<SqChunkArray<T>>>, OutOfBounds> {
        self.0.get_mut(idx).ok_or(OutOfBounds)
    }

    pub(crate) fn get_layer(
        &self,
        idx: usize,
    ) -> Result<&Option<Box<SqChunkArray<T>>>, OutOfBounds> {
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
                inner[x][z].as_mut().map(|slot| *slot = data);
            }
            None => {
                let mut new_layer = SqChunkArray::<T>::default();
                new_layer[x][z].as_mut().map(|slot| *slot = data);
                self.insert_layer(new_layer, y).unwrap();
            }
        }

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

    pub fn get(&self, pos: IVec3) -> Result<Option<T>, OutOfBounds>
    where
        T: Copy,
    {
        if !Self::contains(pos) {
            return Err(OutOfBounds);
        }

        Ok(self.0.get(&pos).copied())
    }
}
