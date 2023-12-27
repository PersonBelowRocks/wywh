use bevy::math::Vec2;
use ordered_float::NotNan;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct NotNanVec2 {
    x: NotNan<f32>,
    y: NotNan<f32>
}

impl std::ops::Deref for NotNanVec2 {
    type Target = Vec2;

    fn deref(&self) -> &Self::Target {
        // SAFETY: idk, maybe? figure it out later lol
        unsafe { std::mem::transmute(self) }
    }
}