use glam::IVec3;
use rangemap::RangeSet;

use crate::Region;

// TODO: removal methods and benchmarks

/// A set of voxel positions. Like a hashset but supports more specialized operations.
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

    #[inline]
    fn insert_internal(&mut self, region: Region) {
        let min = region.min();
        let max = region.max();

        self.x.insert(min.x..max.x);
        self.y.insert(min.y..max.y);
        self.z.insert(min.z..max.z);
    }

    /// Add a position to the set.
    #[inline]
    pub fn insert(&mut self, pos: IVec3) {
        let unit_region = Region::new_inclusive(pos, pos);
        self.insert_internal(unit_region);
    }

    /// Add a region of voxels to the set.
    ///
    /// This is significantly faster than looping over the region and adding its positions individually.
    #[inline]
    pub fn insert_region(&mut self, region: Region) {
        self.insert_internal(region);
    }

    /// Check if the position is present in this set.
    #[inline]
    #[must_use]
    pub fn contains(&self, pos: IVec3) -> bool {
        self.x.contains(&pos.x) && self.y.contains(&pos.y) && self.z.contains(&pos.z)
    }

    /// Check if the entire region is fully contained within this set.
    /// That means that all the positions in the region are present in this set.
    #[inline]
    #[must_use]
    pub fn contains_region(&self, region: Region) -> bool {
        let min = region.min();
        let max = region.max();

        self.x.get(&min.x).is_some_and(|r| r.contains(&max.x))
            && self.x.get(&min.y).is_some_and(|r| r.contains(&max.y))
            && self.x.get(&min.z).is_some_and(|r| r.contains(&max.z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::ivec3;

    #[test]
    fn test_single() {
        let mut set = VoxelSet::new();

        set.insert(ivec3(0, 0, 0));
        set.insert(ivec3(0, 1, 0));
        set.insert(ivec3(10, 17, 3));
        set.insert(ivec3(-1, 0, 1));

        assert!(set.contains(ivec3(0, 0, 0)));
        assert!(set.contains(ivec3(0, 1, 0)));
        assert!(set.contains(ivec3(10, 17, 3)));
        assert!(set.contains(ivec3(-1, 0, 1)));
    }

    #[test]
    #[should_panic]
    fn test_insert_max() {
        let mut set = VoxelSet::new();
        set.insert(ivec3(i32::MAX, 0, 0));
    }

    #[test]
    fn test_region() {
        let mut set = VoxelSet::new();

        set.insert_region(Region::new([0, 0, 0], [5, 5, 5]));
        assert!(set.contains(ivec3(0, 0, 0)));
        assert!(!set.contains(ivec3(5, 5, 5)));
        assert!(!set.contains(ivec3(4, 5, 4)));
        assert!(set.contains(ivec3(4, 4, 4)));
        assert!(!set.contains(ivec3(2, -4, 2)));

        assert!(!set.contains_region(Region::new([0, 0, 0], [5, 5, 5])));
        assert!(set.contains_region(Region::new([0, 0, 0], [4, 4, 4])));
        assert!(set.contains_region(Region::new([1, 1, 2], [3, 2, 3])));

        assert!(!set.contains_region(Region::new([0, -5, 0], [2, 2, 3])));
    }
}
