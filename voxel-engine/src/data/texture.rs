use bevy::math::{vec2, Vec2};
use ordered_float::NotNan;

use crate::util::notnan_arr;

#[derive(Default, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FaceTextureRotation {
    #[default]
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
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
