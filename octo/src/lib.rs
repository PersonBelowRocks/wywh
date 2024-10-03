extern crate ndarray as nda;

pub mod octree;
pub mod region;
pub mod subdiv;
pub mod voxelmap;
pub use region::*;

pub use subdiv::*;

/// Calculate the "remainder" of `x / n^2`. It's not actually the remainder, and
/// this operation is not the same as, say, `rem_euclid` or `%` (at least I think so).
#[inline]
const fn rem_2_pow_n(x: i32, n: u32) -> i32 {
    let pow = 0b1 << n;
    x & ((pow - 1) as i32)
}

/// Calculate the floor of `x / n^2`.
#[inline]
const fn div_2_pow_n(x: i32, n: u32) -> i32 {
    x >> n as i32
}

/// Calculate the "remainder" of `x / n^2`. It's not actually the remainder, and
/// this operation is not the same as, say, `rem_euclid` or `%` (at least I think so).
#[inline]
pub const fn urem_2_pow_n(x: u32, n: u32) -> u32 {
    let pow = 0b1 << n;
    x & (pow - 1)
}

/// Calculate the floor of `x / n^2`.
#[inline]
pub const fn udiv_2_pow_n(x: u32, n: u32) -> u32 {
    x >> n
}
