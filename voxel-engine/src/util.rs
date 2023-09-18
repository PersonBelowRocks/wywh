use bevy::prelude::*;

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

#[derive(Copy, Clone, Debug, dm::Display, PartialEq, Eq)]
pub enum Axis3D {
    X,
    Y,
    Z,
}
