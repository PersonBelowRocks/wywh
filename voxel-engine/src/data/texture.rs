use std::ops;

use bevy::math::{vec2, Vec2};
use ordered_float::NotNan;

use crate::util::notnan_arr;

#[derive(
    Default, Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize,
)]
pub struct FaceTextureRotation(u8);

impl ops::Add<Self> for FaceTextureRotation {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self((self.0 + rhs.0).rem_euclid(Self::TOTAL_ROTATIONS as _))
    }
}

impl FaceTextureRotation {
    pub const TOTAL_ROTATIONS: i32 = 4;
    pub const ONE_TURN_DEG: i32 = 90;
    pub const ONE_TURN_RAD: f32 = 1.57079633;

    pub fn from_str(string: &str) -> Option<Self> {
        todo!()
    }

    pub fn new(value: i32) -> Self {
        let value: u32 = value.rem_euclid(Self::TOTAL_ROTATIONS) as _;
        debug_assert!(value < Self::TOTAL_ROTATIONS as u32);

        Self(value as _)
    }

    pub fn rotate_by(self, rot: i32) -> Self {
        let new_rotation = self.0 as i32 + rot;
        Self::new(new_rotation)
    }

    pub fn degrees(self) -> i32 {
        self.0 as i32 * Self::ONE_TURN_DEG
    }

    pub fn radians(self) -> f32 {
        self.0 as f32 * Self::ONE_TURN_RAD
    }

    pub fn inner(self) -> u8 {
        self.0
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
// TODO: is this worth it? it cuts the size of BlockModel down a lot...
#[repr(packed)]
pub struct FaceTexture {
    pub rotation: FaceTextureRotation,
    tex_pos_x: NotNan<f32>,
    tex_pos_y: NotNan<f32>,
}

impl FaceTexture {
    pub fn tex_pos(&self) -> Vec2 {
        vec2(self.tex_pos_x.into_inner(), self.tex_pos_y.into_inner())
    }

    pub fn new(tex_pos: Vec2) -> Self {
        let [tex_pos_x, tex_pos_y] = notnan_arr(tex_pos.into()).unwrap();

        Self {
            tex_pos_x,
            tex_pos_y,
            rotation: Default::default(),
        }
    }

    pub fn new_rotated(tex_pos: Vec2, rotation: FaceTextureRotation) -> Self {
        let [tex_pos_x, tex_pos_y] = notnan_arr(tex_pos.into()).unwrap();
        Self {
            tex_pos_x,
            tex_pos_y,
            rotation,
        }
    }
}
