use rangemap::RangeSet;

/// A set of voxel positions. Like a hashmap but supports more specialized operations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VoxelSet {
    x: RangeSet<i32>,
    y: RangeSet<i32>,
    z: RangeSet<i32>,
}

impl VoxelSet {
    /// Create a new and empty voxel set.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
