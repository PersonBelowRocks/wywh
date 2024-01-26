pub mod anon;
pub mod data;
pub mod error;
pub mod isometric;

pub use anon::*;
use bevy::{ecs::component::Component, math::Vec3, render::render_resource::ShaderType};
pub use data::*;
pub use error::*;
pub use isometric::*;

#[rustfmt::skip]
pub mod consts {
    pub const ROTATION_MASK: u32 = 0b00000000_00000000_00000000_00000011;
    pub const FLIP_UV_X: u32     = 0b00000000_00000000_00000000_00000100;
    pub const FLIP_UV_Y: u32     = 0b00000000_00000000_00000000_00001000;
    pub const OCCLUSION: u32     = 0b00000000_00000000_00000000_00010000;
}

#[derive(Copy, Clone, Debug, ShaderType, PartialEq)]
pub struct GpuQuad {
    pub texture_id: u32,
    pub rotation: u32,
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Clone, Component)]
pub struct ChunkQuads {
    pub quads: Vec<GpuQuad>,
}
