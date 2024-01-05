use bevy::math::Vec2;

use crate::util::notnan::NotNanVec2;

use super::error::QuadError;

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct Quad {
    dims: NotNanVec2,
}

impl Quad {
    #[inline]
    pub fn new(dims: Vec2) -> Result<Self, QuadError> {
        if dims.x <= 0.0 || dims.y <= 0.0 {
            return Err(QuadError::InvalidDimensions);
        }

        Ok(Self {
            dims: dims.try_into()?,
        })
    }

    #[inline]
    pub fn widened(self, by: f32) -> Result<Self, QuadError> {
        let mut v = self.dims.vec();
        v.x += by;

        Self::new(v.max(Vec2::ZERO))
    }

    #[inline]
    pub fn heightened(self, by: f32) -> Result<Self, QuadError> {
        let mut v = self.dims.vec();
        v.y += by;

        Self::new(v.max(Vec2::ZERO))
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
