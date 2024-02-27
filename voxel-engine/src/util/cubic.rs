use std::array;

use bevy::math::UVec3;

use super::{uvec_to_usize_arr, CubicArray};

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Cubic<const S: usize, T>(CubicArray<S, T>);

impl<const S: usize, T: Default + Copy> Default for Cubic<S, T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<const S: usize, T> Cubic<S, T> {
    pub fn new(fill: T) -> Self
    where
        T: Copy,
    {
        Self([[[fill; S]; S]; S])
    }

    pub fn from_array(arr: CubicArray<S, T>) -> Self {
        Self(arr)
    }

    pub fn contains(pos: UVec3) -> bool {
        pos.cmplt(UVec3::splat(S as u32)).all()
    }

    pub fn get(&self, pos: UVec3) -> Option<&T> {
        if !Self::contains(pos) {
            None
        } else {
            let [x, y, z] = uvec_to_usize_arr(pos);

            Some(&self.0[x][y][z])
        }
    }

    pub fn get_mut(&mut self, pos: UVec3) -> Option<&mut T> {
        if !Self::contains(pos) {
            None
        } else {
            let [x, y, z] = uvec_to_usize_arr(pos);

            Some(&mut self.0[x][y][z])
        }
    }
}

impl<const S: usize, T> Cubic<S, Option<T>> {
    pub fn all_none() -> Self {
        Self::from_array(array::from_fn(|_| {
            array::from_fn(|_| array::from_fn(|_| None))
        }))
    }
}
