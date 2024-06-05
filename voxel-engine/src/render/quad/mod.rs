pub mod anon;
pub mod data;
pub mod error;
pub mod isometric;

use std::{fmt::Debug, mem::size_of};

pub use anon::*;
use bevy::{math::Vec2, render::render_resource::ShaderType};
pub use data::*;
pub use error::*;
pub use isometric::*;
use num_traits::FromPrimitive;

use crate::data::{texture::FaceTextureRotation, tile::Face};

#[rustfmt::skip]
pub mod consts {
    pub const ROTATION_MASK: u32 = 0b00000000_00000000_00000000_00000011;
    pub const FLIP_UV_X: u32     = 0b00000000_00000000_00000000_00000100;
    pub const FLIP_UV_Y: u32     = 0b00000000_00000000_00000000_00001000;
    pub const OCCLUSION: u32     = 0b00000000_00000000_00000000_00010000;
}

#[derive(Copy, Clone, Debug, ShaderType, PartialEq)]
#[repr(C)]
pub struct GpuQuad {
    pub texture_id: u32,
    pub bitfields: GpuQuadBitfields,
    pub min: Vec2,
    pub max: Vec2,
    pub magnitude: i32,
}

#[derive(Copy, Clone, Debug, ShaderType, PartialEq, Eq)]
#[repr(C)]
pub struct GpuQuadBitfields {
    value: u32,
}

impl GpuQuadBitfields {
    pub const ROTATION_MASK: u32 = 0b11 << 0;
    pub const ROTATION_SHIFT: u32 = 0;
    pub const FACE_MASK: u32 = 0b111 << 2;
    pub const FACE_SHIFT: u32 = 2;

    pub const FLIP_UV_X_BIT: u32 = 5;
    pub const FLIP_UV_Y_BIT: u32 = 6;

    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn get_face(self) -> Face {
        let raw = (self.value & Self::FACE_MASK) >> Self::FACE_SHIFT;
        FromPrimitive::from_u32(raw).unwrap()
    }

    pub fn with_rotation(mut self, rotation: FaceTextureRotation) -> Self {
        self.value |= (rotation.inner() as u32) << Self::ROTATION_SHIFT;
        self
    }

    pub fn with_face(mut self, face: Face) -> Self {
        self.value |= face.as_u32() << Self::FACE_SHIFT;
        self
    }

    pub fn with_flip_x(mut self, flip: bool) -> Self {
        if flip {
            self.value |= 0b1 << Self::FLIP_UV_X_BIT;
        }
        self
    }

    pub fn with_flip_y(mut self, flip: bool) -> Self {
        if flip {
            self.value |= 0b1 << Self::FLIP_UV_Y_BIT;
        }
        self
    }
}

#[derive(Clone)]
pub struct ChunkQuads {
    pub quads: Vec<GpuQuad>,
}

impl ChunkQuads {
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }
}

impl Debug for ChunkQuads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuQuads")
            .field("quad_count", &self.quads.len())
            .field("capacity", &self.quads.capacity())
            .field(
                "bytes_used",
                &(self.quads.capacity() * size_of::<GpuQuad>()),
            )
            .finish()
    }
}
