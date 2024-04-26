use bevy::math::{ivec2, ivec3, IVec2, IVec3};

use crate::{data::tile::Face, util::Axis3D};

pub mod access;
pub mod block;
pub mod bounding_box;
pub mod ecs;
pub mod error;
pub mod neighbors;
pub mod storage;
pub mod util;
pub mod world;
pub mod worldgen;

pub use util::*;

#[inline]
pub fn ivec_project_to_3d(pos: IVec2, face: Face, mag: i32) -> IVec3 {
    match face.axis() {
        Axis3D::X => ivec3(mag, pos.y, pos.x),
        Axis3D::Y => ivec3(pos.x, mag, pos.y),
        Axis3D::Z => ivec3(pos.x, pos.y, mag),
    }
}

#[inline]
pub fn ivec_project_to_2d(pos: IVec3, face: Face) -> IVec2 {
    match face.axis() {
        Axis3D::X => ivec2(pos.z, pos.y),
        Axis3D::Y => ivec2(pos.x, pos.z),
        Axis3D::Z => ivec2(pos.x, pos.y),
    }
}
