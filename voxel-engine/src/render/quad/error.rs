#[derive(te::Error, Debug, Clone)]
pub enum QuadError {
    #[error("Quad dimensions were invalid, width and height must both be greater than 0")]
    InvalidDimensions
}
