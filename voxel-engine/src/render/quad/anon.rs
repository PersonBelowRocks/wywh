use bevy::math::Vec2;
use ordered_float::FloatIsNan;

use crate::util::notnan::NotNanVec2;

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct Quad {
    dims: NotNanVec2,
}

impl Quad {
    #[inline]
    pub fn new(dims: Vec2) -> Result<Self, FloatIsNan> {
        Ok(Self {
            dims: dims.try_into()?,
        })
    }

    #[inline]
    pub fn dims(self) -> Vec2 {
        self.dims.vec()
    }

    #[inline]
    pub fn x(self) -> f32 {
        self.dims().x
    }

    #[inline]
    pub fn y(self) -> f32 {
        self.dims().y
    }
}
