use bevy::prelude::*;
use dashmap::DashMap;
use std::{array, fmt::Debug};

use crate::data::tile::Face;

pub type SyncHashMap<K, V> = DashMap<K, V, ahash::RandomState>;
pub type SquareArray<const SIZE: usize, T> = [[T; SIZE]; SIZE];
pub type CubicArray<const SIZE: usize, T> = [[[T; SIZE]; SIZE]; SIZE];

#[derive(te::Error, Debug, PartialEq, Eq)]
#[error("Could not convert vector {0}")]
pub struct ConversionError(IVec3);

pub fn try_ivec3_to_usize_arr(ivec: IVec3) -> Result<[usize; 3], ConversionError> {
    let [x, y, z] = ivec.to_array();

    Ok([
        x.try_into().map_err(|_| ConversionError(ivec))?,
        y.try_into().map_err(|_| ConversionError(ivec))?,
        z.try_into().map_err(|_| ConversionError(ivec))?,
    ])
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis2D {
    X,
    Y,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis3D {
    X,
    Y,
    Z,
}

impl Axis3D {
    pub const XYZ: [Self; 3] = [Self::X, Self::Y, Self::Z];

    pub fn choose(self, vec: Vec3) -> f32 {
        match self {
            Self::X => vec.x,
            Self::Y => vec.y,
            Self::Z => vec.z,
        }
    }

    pub fn pos_in_3d(self, pos: IVec2, magnitude: i32) -> IVec3 {
        match self {
            Self::X => [magnitude, pos.x, pos.y],
            Self::Y => [pos.x, magnitude, pos.y],
            Self::Z => [pos.x, pos.y, magnitude],
        }
        .into()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisMagnitude {
    pub magnitude: f32,
    pub axis: Axis3D,
}

impl AxisMagnitude {
    pub fn new(axis: Axis3D, magnitude: f32) -> Self {
        Self { magnitude, axis }
    }

    pub fn add_magnitude(mut self, mag: f32) -> Self {
        self.magnitude += mag;
        self
    }
}

#[derive(Copy, Clone)]
pub struct FaceMap<T>([Option<T>; 6]);

impl<T> FaceMap<T> {
    pub fn new() -> Self {
        Self(array::from_fn(|_| None))
    }

    pub fn get(&self, face: Face) -> Option<&T> {
        use num_traits::ToPrimitive;

        self.0[face.to_usize().unwrap()].as_ref()
    }

    pub fn get_mut(&mut self, face: Face, data: T) {
        use num_traits::ToPrimitive;

        self.0[face.to_usize().unwrap()] = Some(data)
    }
}

impl<T: Copy> FaceMap<T> {
    pub fn filled(data: T) -> Self {
        Self([Some(data); 6])
    }
}

impl<T: Debug> Debug for FaceMap<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();

        for face in Face::FACES {
            if let Some(v) = self.get(face) {
                map.entry(&face, v);
            }
        }

        map.finish()
    }
}

pub fn circular_shift<T: Copy, const C: usize>(arr: [T; C], shift: isize) -> [T; C] {
    let mut out = arr;

    for (i, &e) in arr.iter().enumerate() {
        let shifted_index = (i as isize + shift).rem_euclid(C as isize);

        out[shifted_index as usize] = e;
    }

    out
}

pub trait ArrayExt {
    fn circular_shift(self, shift: isize) -> Self;
    fn reversed(self) -> Self;
}

impl<T: Copy, const SIZE: usize> ArrayExt for [T; SIZE] {
    fn circular_shift(self, shift: isize) -> Self {
        circular_shift(self, shift)
    }

    fn reversed(mut self) -> Self {
        self.reverse();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circular_shift() {
        let arr = [0, 1, 2, 3];

        assert_eq!([3, 0, 1, 2], circular_shift(arr, 1));
        assert_eq!([1, 2, 3, 0], circular_shift(arr, -1));
        assert_eq!([2, 3, 0, 1], circular_shift(arr, 2));
        assert_eq!([2, 3, 0, 1], circular_shift(arr, -2));
        assert_eq!([1, 2, 3, 0], circular_shift(arr, 3));
        assert_eq!([3, 0, 1, 2], circular_shift(arr, -3));
    }

    #[test]
    fn test_circular_shift_modulo() {
        let arr = [0, 1, 2, 3];

        assert_eq!([2, 3, 0, 1], circular_shift(arr, 6));
        assert_eq!([2, 3, 0, 1], circular_shift(arr, -6));

        assert_eq!([1, 2, 3, 0], circular_shift(arr, 7));
        assert_eq!([3, 0, 1, 2], circular_shift(arr, -7));
    }

    #[test]
    fn test_circular_shift_unchanged() {
        let arr = [0, 1, 2, 3];

        assert_eq!(arr, circular_shift(arr, 4));
        assert_eq!(arr, circular_shift(arr, -4));

        assert_eq!(arr, circular_shift(arr, 0));
        assert_eq!(arr, circular_shift(arr, -0));
    }
}
