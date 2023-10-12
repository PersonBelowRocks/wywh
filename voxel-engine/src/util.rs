use bevy::{prelude::*, render::render_resource::encase::vector::VectorScalar};

#[derive(te::Error, Debug, PartialEq, Eq)]
#[error("Could not convert vector {0}")]
pub struct ConversionError(IVec3);

pub fn try_ivec3_to_usize_arr(ivec: IVec3) -> Result<[usize; 3], ConversionError> {
    let [x, y, z] = ivec.to_array();

    Ok([
        x.try_into().map_err(|_| ConversionError(ivec))?,
        y.try_into().map_err(|_| ConversionError(ivec))?,
        z.try_into().map_err(|_| ConversionError(ivec))?,
    ])
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis3D {
    X,
    Y,
    Z,
}

impl Axis3D {
    pub const XYZ: [Self; 3] = [Self::X, Self::Y, Self::Z];

    pub fn choose(self, vec: Vec3) -> f32 {
        match self {
            Self::X => vec.x,
            Self::Y => vec.y,
            Self::Z => vec.z,
        }
    }

    pub fn pos_in_3d(self, pos: IVec2, magnitude: i32) -> IVec3 {
        match self {
            Self::X => [magnitude, pos.x, pos.y],
            Self::Y => [pos.x, magnitude, pos.y],
            Self::Z => [pos.x, pos.y, magnitude],
        }
        .into()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AxisMagnitude {
    pub magnitude: f32,
    pub axis: Axis3D,
}

impl AxisMagnitude {
    pub fn new(axis: Axis3D, magnitude: f32) -> Self {
        Self { magnitude, axis }
    }

    pub fn add_magnitude(mut self, mag: f32) -> Self {
        self.magnitude += mag;
        self
    }
}
