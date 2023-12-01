use bevy::math::{vec2, Vec2};
use ordered_float::NotNan;

use crate::util::notnan_arr;

#[derive(
    Default, Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize,
)]
pub enum FaceTextureRotation {
    #[default]
    #[serde(rename = "u")]
    Up = 0,
    #[serde(rename = "d")]
    Down = 1,
    #[serde(rename = "l")]
    Left = 2,
    #[serde(rename = "r")]
    Right = 3,
}

impl FaceTextureRotation {
    pub fn from_str(string: &str) -> Option<Self> {
        match string {
            "u" | "up" => Some(Self::Up),
            "d" | "down" => Some(Self::Down),
            "l" | "left" => Some(Self::Left),
            "r" | "right" => Some(Self::Right),
            _ => None,
        }
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
