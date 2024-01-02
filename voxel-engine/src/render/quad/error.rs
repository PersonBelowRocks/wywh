use ordered_float::FloatIsNan;

#[derive(te::Error, Debug, Clone)]
pub enum QuadError {
    #[error("Cannot use NaN floats in quads")]
    FloatIsNan(#[from] FloatIsNan),
    #[error("Quad dimensions were invalid, width and height must be greater than 0.0")]
    InvalidDimensions,
}
