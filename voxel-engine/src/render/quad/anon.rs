use std::num::NonZeroU32;

use bevy::math::{ivec2, IVec2, UVec2};

use super::error::QuadError;

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct Quad {
    x: NonZeroU32,
    y: NonZeroU32,
}

impl Quad {
    pub const ONE: Self = Self {
        // SAFETY: the u32 literal is not zero
        x: unsafe { NonZeroU32::new_unchecked(1) },
        y: unsafe { NonZeroU32::new_unchecked(1) },
    };

    #[inline]
    pub fn new(dims: UVec2) -> Result<Self, QuadError> {
        if dims.x == 0 || dims.y == 0 {
            return Err(QuadError::InvalidDimensions);
        }

        Ok(Self {
            x: dims.x.try_into().unwrap(),
            y: dims.y.try_into().unwrap(),
        })
    }

    #[inline]
    pub fn widened(self, by: i32) -> Result<Self, QuadError> {
        let new_x = u32::from(self.x) as i32 + by;
        if new_x <= 0 {
            return Err(QuadError::InvalidDimensions);
        }

        Ok(Self {
            x: NonZeroU32::new(new_x as u32).unwrap(),
            y: self.y,
        })
    }

    #[inline]
    pub fn heightened(self, by: i32) -> Result<Self, QuadError> {
        let new_y = u32::from(self.y) as i32 + by;
        if new_y <= 0 {
            return Err(QuadError::InvalidDimensions);
        }

        Ok(Self {
            x: self.x,
            y: NonZeroU32::new(new_y as u32).unwrap(),
        })
    }

    #[inline]
    pub fn dims(self) -> IVec2 {
        ivec2(self.x(), self.y())
    }

    #[inline]
    pub fn x(self) -> i32 {
        u32::from(self.x) as i32
    }

    #[inline]
    pub fn y(self) -> i32 {
        u32::from(self.y) as i32
    }
}
