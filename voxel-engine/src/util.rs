use bevy::prelude::*;

#[derive(te::Error, Debug)]
#[error("Could not convert number")]
pub struct ConversionError;

pub fn try_ivec3_to_usize_arr(ivec: IVec3) -> Result<[usize; 3], ConversionError> {
    let [x, y, z] = ivec.to_array();

    Ok([
        x.try_into().map_err(|_| ConversionError)?,
        y.try_into().map_err(|_| ConversionError)?,
        z.try_into().map_err(|_| ConversionError)?,
    ])
}
