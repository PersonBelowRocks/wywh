use bevy::math::{vec2, Vec2};
use ordered_float::{FloatIsNan, NotNan};

#[derive(
    Copy, Clone, Hash, PartialEq, Eq, Debug, dm::Display, serde::Deserialize, serde::Serialize,
)]
#[display("{}", "self.vec()")]
pub struct NotNanVec2 {
    pub x: NotNan<f32>,
    pub y: NotNan<f32>,
}

impl NotNanVec2 {
    pub const ONE: Self = Self {
        // SAFETY: the float literal is not NaN
        x: unsafe { NotNan::new_unchecked(1.0) },
        y: unsafe { NotNan::new_unchecked(1.0) },
    };

    pub fn new(vec: Vec2) -> Result<Self, FloatIsNan> {
        Ok(Self {
            x: NotNan::new(vec.x)?,
            y: NotNan::new(vec.y)?,
        })
    }

    pub fn vec(self) -> Vec2 {
        vec2(self.x.into(), self.y.into())
    }
}

impl TryFrom<Vec2> for NotNanVec2 {
    type Error = FloatIsNan;

    fn try_from(value: Vec2) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<NotNanVec2> for Vec2 {
    fn from(value: NotNanVec2) -> Self {
        value.vec()
    }
}
