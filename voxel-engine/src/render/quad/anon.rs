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
    pub fn widened(self, by: f32) -> Self {
        let mut v = self.dims.vec();
        v.x += by;

        Self::new(v.max(Vec2::ZERO)).unwrap()
    }

    #[inline]
    pub fn heightened(self, by: f32) -> Self {
        let mut v = self.dims.vec();
        v.y += by;

        Self::new(v.max(Vec2::ZERO)).unwrap()
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
