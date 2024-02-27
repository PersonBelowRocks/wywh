pub mod cubic;
pub mod notnan;
pub mod result;

use bevy::prelude::*;
use dashmap::DashMap;
use ordered_float::NotNan;
use std::{array, fmt::Debug, marker::PhantomData};

use crate::data::tile::Face;

pub type SyncHashMap<K, V> = DashMap<K, V, ahash::RandomState>;
pub type SquareArray<const SIZE: usize, T> = [[T; SIZE]; SIZE];
pub type CubicArray<const SIZE: usize, T> = [[[T; SIZE]; SIZE]; SIZE];

#[derive(te::Error, Debug, PartialEq, Eq)]
#[error("Could not convert vector {0}")]
pub struct ConversionError(IVec3);

pub fn notnan_arr<const SIZE: usize>(arr: [f32; SIZE]) -> Option<[NotNan<f32>; SIZE]> {
    if arr.iter().any(|f| f.is_nan()) {
        return None;
    }

    Some(arr.map(|f| NotNan::new(f).unwrap()))
}

pub fn ivec3_to_1d(v: IVec3, max: usize) -> Result<usize, ConversionError> {
    let [x, y, z] = try_ivec3_to_usize_arr(v)?;
    Ok(to_1d(x, y, z, max))
}

pub fn to_1d(x: usize, y: usize, z: usize, max: usize) -> usize {
    return (z * max * max) + (y * max) + x;
}

pub fn try_ivec3_to_usize_arr(ivec: IVec3) -> Result<[usize; 3], ConversionError> {
    let [x, y, z] = ivec.to_array();

    Ok([
        x.try_into().map_err(|_| ConversionError(ivec))?,
        y.try_into().map_err(|_| ConversionError(ivec))?,
        z.try_into().map_err(|_| ConversionError(ivec))?,
    ])
}

pub fn uvec_to_usize_arr(uvec: UVec3) -> [usize; 3] {
    let [x, y, z] = uvec.to_array();

    [x as usize, y as usize, z as usize]
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis2D {
    X,
    Y,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis3D {
    X = 0,
    Y = 1,
    Z = 2,
}

impl Axis3D {
    pub const XYZ: [Self; 3] = [Self::X, Self::Y, Self::Z];

    pub fn as_usize(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

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

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub struct FaceMap<T>([Option<T>; 6]);

#[derive(Clone)]
pub struct FaceMapIterator<'a, T> {
    map: &'a FaceMap<T>,
    face_iter: std::slice::Iter<'static, Face>,
}

impl<'a, T> Iterator for FaceMapIterator<'a, T> {
    type Item = (Face, Option<&'a T>);

    fn next(&mut self) -> Option<Self::Item> {
        let face = *self.face_iter.next()?;
        Some((face, self.map.get(face)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (Face::FACES.len(), Some(Face::FACES.len()))
    }
}

impl<T> Default for FaceMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FaceMap<T> {
    pub fn new() -> Self {
        Self(array::from_fn(|_| None))
    }

    pub fn from_fn<F: FnMut(Face) -> Option<T>>(f: F) -> Self {
        Self(Face::FACES.map(f))
    }

    pub fn iter(&self) -> FaceMapIterator<'_, T> {
        FaceMapIterator {
            map: self,
            face_iter: Face::FACES.iter(),
        }
    }

    pub fn all<F: FnMut(Face, &T) -> bool>(&self, mut f: F) -> bool {
        for face in Face::FACES {
            if let Some(value) = self.get(face) {
                if !f(face, value) {
                    return false;
                }
            }
        }

        true
    }

    pub fn any<F: FnMut(Face, &T) -> bool>(&self, mut f: F) -> bool {
        for face in Face::FACES {
            if let Some(value) = self.get(face) {
                if f(face, value) {
                    return true;
                }
            }
        }

        false
    }

    pub fn get(&self, face: Face) -> Option<&T> {
        use num_traits::ToPrimitive;

        self.0[face.to_usize().unwrap()].as_ref()
    }

    pub fn set(&mut self, face: Face, data: T) -> Option<T> {
        use num_traits::ToPrimitive;

        self.0[face.to_usize().unwrap()].replace(data)
    }

    pub fn remove(&mut self, face: Face) -> Option<T> {
        use num_traits::ToPrimitive;

        self.0[face.to_usize().unwrap()].take()
    }

    pub fn map<U, F: FnMut(Face, &T) -> U>(&self, mut f: F) -> FaceMap<U> {
        let mut mapped = FaceMap::<U>::new();

        for face in Face::FACES {
            if let Some(data) = self.get(face) {
                mapped.set(face, f(face, data));
            }
        }

        mapped
    }

    pub fn len(&self) -> usize {
        self.0.iter().filter(|&v| v.is_some()).count()
    }

    pub fn is_filled(&self) -> bool {
        self.len() == 6
    }
}

impl<T: serde::Serialize> serde::Serialize for FaceMap<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut sermap = serializer.serialize_map(Some(self.len()))?;

        self.map(|face, value| sermap.serialize_entry(&face, value));

        sermap.end()
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for FaceMap<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(FaceMapVisitor(PhantomData))
    }
}

struct FaceMapVisitor<T>(PhantomData<T>);

impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for FaceMapVisitor<T> {
    type Value = FaceMap<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map with keyed with the faces of a cube")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut out = FaceMap::<T>::new();

        while let Some((face, value)) = map.next_entry::<Face, T>()? {
            out.set(face, value);
        }

        Ok(out)
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

    #[test]
    fn test_facemap_deserialize() {
        let raw = r#"
            {
                north: 42,
                w: 12,
                east: 100,
                b: 0,
            }
        "#;

        let map = deser_hjson::from_str::<FaceMap<u32>>(raw).unwrap();
        let mut expected_map = FaceMap::new();
        expected_map.set(Face::North, 42);
        expected_map.set(Face::West, 12);
        expected_map.set(Face::East, 100);
        expected_map.set(Face::Bottom, 0);

        assert_eq!(expected_map, map);
    }

    // TODO: FaceMap serialization test
}
