use crate::data::registries::block::BlockVariantId;
use bevy::math::IVec3;
use std::fmt::Debug;

/// This trait generalizes the behaviour of a chunk, allowing for data to be arranged in such a way
/// that it can play the role of a chunk without actually being one.
///
/// This is mainly used for mocking chunks in testing, but can also be used in real code.
pub trait GenericChunkReadAccess {
    /// The error to be returned by getters. Must be able to represent an out-of-bounds error.
    type Error: OutOfBoundsError + Debug;

    /// Get a full-block from the chunk. If the block at the requested position is not a full-block,
    /// [`None`] must be returned.
    ///
    /// The given position must be in local full-block space, and must be confined to a chunk (16x16x16).
    /// Any position that does not meet these criteria must be rejected with an out-of-bounds error.
    fn get(&self, ls_pos: IVec3) -> Result<Option<BlockVariantId>, Self::Error>;

    /// Get a microblock from the chunk. Unlike the normal [`GenericChunkReadAccess::get`] method, this
    /// one can't return [`None`].
    ///
    /// The given position must be in local microblock space, and must be confined to a subdivided chunk (64x64x64).
    /// Any position that does not meet these criteria must be rejected with an out-of-bounds error.
    fn get_mb(&self, mb_pos: IVec3) -> Result<BlockVariantId, Self::Error>;
}

/// Trait implemented by error types that can represent an "out-of-bounds" error.
pub trait OutOfBoundsError {
    /// Returns `true` if this error represents an out-of-bounds error.
    fn is_out_of_bounds(&self) -> bool;
}

/// Implement the [`OutOfBoundsError`] trait for an error enum. An error variant will be considered
/// out-of-bounds if it matches the given pattern.
#[macro_export]
macro_rules! enum_implement_oob_error {
    ($t:ty, $pattern:pat) => {
        impl $crate::topo::generic_chunk::OutOfBoundsError for $t {
            fn is_out_of_bounds(&self) -> bool {
                matches!(self, $pattern)
            }
        }
    };
}
