use std::{mem::size_of, num::NonZeroU8};

use bevy::{ecs::component::Component, math::IVec3};

use crate::{
    data::tile::Face,
    topo::{
        access::{HasBounds, ReadAccess, WriteAccess},
        bounding_box::BoundingBox,
        chunk::Chunk,
        storage::error::OutOfBounds,
    },
    util::ivec3_to_1d,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct BlockOcclusion(Option<NonZeroU8>);

static_assertions::assert_eq_size!(u8, BlockOcclusion);

impl BlockOcclusion {
    pub(crate) const MASK: u8 = 0b00111111;

    pub fn new(faces: &[Face]) -> Self {
        let mut value: u8 = 0;

        for face in faces {
            let face_idx = face.as_usize() as u8;
            value |= 1u8 << face_idx;
        }

        Self(NonZeroU8::new(value))
    }

    pub fn empty() -> Self {
        Self(None)
    }

    pub fn filled() -> Self {
        Self(NonZeroU8::new(Self::MASK))
    }

    pub fn is_occluded(&self, face: Face) -> bool {
        match self.0 {
            Some(v) => u8::from(v) & (face.as_usize() as u8) != 0,
            None => false,
        }
    }

    pub fn as_byte(self) -> u8 {
        self.0.map(|b| u8::from(b)).unwrap_or(0)
    }

    pub fn from_byte(mut byte: u8) -> Self {
        byte &= 0b00111111;
        Self(NonZeroU8::new(byte))
    }
}

#[derive(Clone, Debug, Component)]
pub struct ChunkOcclusionMap([BlockOcclusion; Self::BUFFER_SIZE]);

// we need to be able to reinterperet the whole buffer as a buffer of u32s
static_assertions::const_assert_eq!(0, ChunkOcclusionMap::BUFFER_SIZE % size_of::<u32>());

impl ChunkOcclusionMap {
    pub const USIZE: usize = Chunk::USIZE + 2;
    pub const SIZE: i32 = Self::USIZE as i32;

    pub const BUFFER_SIZE: usize = Self::USIZE.pow(3);
    pub const GPU_BUFFER_SIZE: u32 = (Self::BUFFER_SIZE as u32) / 4;
    pub const GPU_BUFFER_DIMENSIONS: u32 = Self::USIZE as u32;

    pub const BOUNDS: BoundingBox = BoundingBox {
        min: IVec3::splat(-1),
        // this is a stupid workaround for arithmetic traits not being usable in const lol
        max: Chunk::VEC.saturating_add(IVec3::ONE),
    };

    pub fn new() -> Self {
        Self([BlockOcclusion::default(); Self::BUFFER_SIZE])
    }

    pub fn as_buffer(self) -> Vec<[u8; size_of::<u32>()]> {
        let mut buffer = vec![[0; size_of::<u32>()]; Self::BUFFER_SIZE / size_of::<u32>()];

        for (i, occlusion_v) in self
            .0
            .chunks(size_of::<u32>())
            .map(|slice| <[BlockOcclusion; size_of::<u32>()]>::try_from(slice).unwrap())
            .enumerate()
        {
            buffer[i] = occlusion_v.map(BlockOcclusion::as_byte);
        }

        buffer
    }
}

pub(crate) fn ivec3_to_cmo_idx(mut pos: IVec3) -> Result<usize, OutOfBounds> {
    if !ChunkOcclusionMap::BOUNDS.contains(pos) {
        return Err(OutOfBounds);
    }

    // the lowest value pos can be is [-1, -1, -1]
    pos += IVec3::ONE;
    ivec3_to_1d(pos, ChunkOcclusionMap::USIZE).map_err(|_| OutOfBounds)
}

impl HasBounds for ChunkOcclusionMap {
    fn bounds(&self) -> BoundingBox {
        Self::BOUNDS
    }
}

impl WriteAccess for ChunkOcclusionMap {
    type WriteErr = OutOfBounds;
    type WriteType = BlockOcclusion;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        let idx = ivec3_to_cmo_idx(pos)?;
        self.0[idx] = data;

        Ok(())
    }
}

impl ReadAccess for ChunkOcclusionMap {
    type ReadErr = OutOfBounds;
    type ReadType = BlockOcclusion;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        let idx = ivec3_to_cmo_idx(pos)?;
        Ok(self.0[idx])
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::ivec3;

    use super::*;

    #[test]
    fn divisible_by_u32_size() {
        let v = ChunkOcclusionMap::BUFFER_SIZE;
        assert!(v % size_of::<u32>() == 0);
    }

    #[test]
    fn test_chunk_occlusion_map() {
        let mut com = ChunkOcclusionMap::new();

        com.set(ivec3(-1, -1, -1), BlockOcclusion::new(&[Face::South]))
            .unwrap();
        assert!(com
            .set(ivec3(-2, -1, -1), BlockOcclusion::new(&[Face::South]))
            .is_err());

        com.set(ivec3(16, 16, 16), BlockOcclusion::new(&[Face::North]))
            .unwrap();
        assert!(com
            .set(ivec3(16, 16, 17), BlockOcclusion::new(&[Face::North]))
            .is_err());

        com.set(ivec3(12, 7, 5), BlockOcclusion::new(&[Face::East]))
            .unwrap();

        assert_eq!(
            BlockOcclusion::new(&[Face::South]),
            com.get(ivec3(-1, -1, -1)).unwrap()
        );
        assert_eq!(
            BlockOcclusion::new(&[Face::North]),
            com.get(ivec3(16, 16, 16)).unwrap()
        );
        assert_eq!(
            BlockOcclusion::new(&[Face::East]),
            com.get(ivec3(12, 7, 5)).unwrap()
        );
    }

    #[test]
    fn test_shader_logic() {
        let mut com = ChunkOcclusionMap::new();
        com.set(ivec3(5, 5, 3), BlockOcclusion::new(&[Face::North]))
            .unwrap();
        com.set(
            ivec3(-1, 15, 6),
            BlockOcclusion::new(&[Face::East, Face::West, Face::Bottom]),
        )
        .unwrap();
        com.set(ivec3(0, 5, 0), BlockOcclusion::new(&[Face::West]))
            .unwrap();
        com.set(ivec3(10, 2, 14), BlockOcclusion::new(&[Face::South]))
            .unwrap();

        let buffer = com.as_buffer();
        let f = |pos: IVec3, occ: BlockOcclusion| {
            let whole_idx = ivec3_to_cmo_idx(pos).unwrap();
            let idx = whole_idx / size_of::<u32>();
            let value_in_shader = u32::from_le_bytes(buffer[idx]);

            let subidx = (whole_idx as u32) % (size_of::<u32>() as u32);
            let mask = (u8::MAX as u32) << (subidx * u8::BITS);

            let normalized_value = (value_in_shader & mask) >> (subidx * u8::BITS);

            if occ.as_byte() as u32 != normalized_value {
                dbg!(whole_idx);
                dbg!(idx);
                dbg!(subidx);
                eprintln!("mask            = {:#032b}", mask);
                eprintln!("value_in_shader = {:#032b}", value_in_shader);
                panic!(
                    "{:#032b} != {:#032b}",
                    occ.as_byte() as u32,
                    normalized_value
                );
            }
        };

        f(ivec3(5, 5, 3), BlockOcclusion::new(&[Face::North]));
        f(
            ivec3(-1, 15, 6),
            BlockOcclusion::new(&[Face::East, Face::West, Face::Bottom]),
        );
        f(ivec3(0, 5, 0), BlockOcclusion::new(&[Face::West]));
        f(ivec3(10, 2, 14), BlockOcclusion::new(&[Face::South]));
    }
}
