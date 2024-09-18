pub mod core;
pub mod lod;
pub mod meshing;
pub mod quad;

pub use core::ChunkHzbOcclusionCulling;

/// Rust versions of WGSL functions, and utilities to bridge the gap between Rust and WGSL.
pub mod wgsl {
    /// Corresponds to WGSL's `roundUp` function.
    ///
    /// From the [WGSL spec][https://www.w3.org/TR/WGSL/#roundup]:
    ///
    /// The `roundUp` function is defined for positive integers `k` and `n` as:
    /// - `roundUp(k, n) = ⌈n ÷ k⌉ × k`
    #[inline]
    pub const fn round_up(k: u64, n: u64) -> u64 {
        u64::div_ceil(n, k) * k
    }
}
