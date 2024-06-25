use bevy::{
    ecs::component::{ComponentHooks, StorageType},
    prelude::Component,
};
use bitflags::bitflags;
use enum_map::{Enum, EnumMap};

/// Level of detail of a chunk mesh.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Enum)]
pub enum LevelOfDetail {
    /// Chunk is rendered as a 1x1x1 cube
    /// Lowest level of detail, only 1 quad per face is allowed, making the entire chunk one big "block"
    X1 = 0,
    /// Chunk is rendered as a 2x2x2 cube
    X2 = 1,
    /// Chunk is rendered as a 4x4x4 cube
    X4 = 2,
    /// Chunk is rendered as a 8x8x8 cube
    X8 = 3,
    /// Chunk is rendered as a 16x16x16 cube without any microblocks
    X16 = 4,
    /// Chunk is rendered as a 16x16x16 cube with microblocks. Highest level of detail.
    /// This is the "true" appearence of a chunk.
    X16Subdiv = 5,
}

/// Maps levels of detail to values of a type.
#[derive(Default, Clone)]
pub struct LodMap<T>(EnumMap<LevelOfDetail, Option<T>>);

impl<T> LodMap<T> {
    pub fn new() -> Self {
        Self(EnumMap::default())
    }

    pub fn get(&self, lod: LevelOfDetail) -> Option<&T> {
        self.0[lod].as_ref()
    }

    pub fn get_mut(&mut self, lod: LevelOfDetail) -> Option<&mut T> {
        self.0[lod].as_mut()
    }

    pub fn insert(&mut self, lod: LevelOfDetail, value: T) -> Option<T> {
        self.0[lod].replace(value)
    }

    pub fn remove(&mut self, lod: LevelOfDetail) -> Option<T> {
        self.0[lod].take()
    }

    pub fn clear(&mut self) {
        for (_, item) in self.0.iter_mut() {
            *item = None;
        }
    }

    pub fn retain(&mut self, lods: LODs) {
        for (lod, item) in self.0.iter_mut() {
            if !lods.contains_lod(lod) {
                *item = None;
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (LevelOfDetail, &T)> {
        self.0
            .iter()
            .filter_map(|(lod, item)| item.as_ref().map(|item| (lod, item)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (LevelOfDetail, &mut T)> {
        self.0
            .iter_mut()
            .filter_map(|(lod, item)| item.as_mut().map(|item| (lod, item)))
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, item)| item)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.iter_mut().map(|(_, item)| item)
    }
}

impl LevelOfDetail {
    pub const LODS: [Self; 6] = [
        Self::X1,
        Self::X2,
        Self::X4,
        Self::X8,
        Self::X16,
        Self::X16Subdiv,
    ];

    /// Returns this LOD as a byte
    #[inline]
    pub const fn as_byte(self) -> u8 {
        self as u8
    }

    /// Returns the bitflag for this LOD
    #[inline]
    pub fn bitflag(self) -> LODs {
        LODs::from_bits(self.as_byte()).expect(
            "The LODs bitflags should contain all possible variants of the LevelOfDetail enum",
        )
    }
}

impl Ord for LevelOfDetail {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

impl PartialOrd for LevelOfDetail {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

bitflags! {
    /// A set of LODs as flags
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
    pub struct LODs: u8 {
        const X1 = 1 << LevelOfDetail::X1.as_byte();
        const X2 = 1 << LevelOfDetail::X2.as_byte();
        const X4 = 1 << LevelOfDetail::X4.as_byte();
        const X8 = 1 << LevelOfDetail::X8.as_byte();
        const X16 = 1 << LevelOfDetail::X16.as_byte();
        const X16SUBDIV = 1 << LevelOfDetail::X16Subdiv.as_byte();
    }
}

impl LODs {
    pub fn from_map<T>(map: &EnumMap<LevelOfDetail, Option<T>>) -> Self {
        let mut new = Self::empty();

        for (lod, value) in map.iter() {
            if value.is_some() {
                new.insert_lod(lod)
            }
        }

        new
    }

    pub fn insert_lod(&mut self, lod: LevelOfDetail) {
        self.insert(lod.bitflag())
    }

    pub fn remove_lod(&mut self, lod: LevelOfDetail) {
        self.remove(lod.bitflag())
    }

    pub fn contains_lod(&self, lod: LevelOfDetail) -> bool {
        self.contains(lod.bitflag())
    }

    pub fn retain_for<T>(&self, map: &mut LodMap<Option<T>>) {
        for (lod, value) in map.iter_mut() {
            if !self.contains_lod(lod) {
                *value = None;
            }
        }
    }

    pub fn contained_lods(&self) -> LodIterator {
        LodIterator {
            lods: *self,
            current: 0,
        }
    }
}

#[derive(Debug)]
pub struct LodIterator {
    lods: LODs,
    current: usize,
}

impl Iterator for LodIterator {
    type Item = LevelOfDetail;

    fn next(&mut self) -> Option<Self::Item> {
        let get = |idx: usize| LevelOfDetail::LODS.get(idx).copied();

        while !self.lods.contains_lod(get(self.current)?) {
            self.current += 1;
        }

        Some(LevelOfDetail::LODS[self.current])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_lod_iterator() {
        todo!()
    }
}
