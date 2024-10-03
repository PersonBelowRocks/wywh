use std::ops::Range;

use glam::{IVec3, UVec3};
use hashbrown::hash_map::IntoIter;

/// A region of voxels.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Region {
    /// The minimum corner of the region.
    ///
    /// For `n` component of a 3D vector, `min[n] <= max[n]`
    min: IVec3,
    /// The maximum corner of the region.
    ///
    /// For `n` component of a 3D vector, `max[n] >= min[n]`
    max: IVec3,
}

impl Region {
    /// Create a new region bounded by the 2 given positions.
    ///
    /// The region will include both of the given positions.
    #[inline]
    pub fn new(a: IVec3, b: IVec3) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// The minimum position of the region.
    #[inline]
    #[must_use]
    pub fn min(self) -> IVec3 {
        self.min
    }

    /// The maximum position of the region.
    #[inline]
    #[must_use]
    pub fn max(self) -> IVec3 {
        self.max
    }

    /// The dimensions of this region.
    #[inline]
    #[must_use]
    pub fn dimensions(self) -> UVec3 {
        (self.max - self.min).as_uvec3()
    }

    /// The volume of this region.
    ///
    /// # Examples
    /// ```
    /// # use glam::{IVec3, ivec3};
    /// # use octo::Region;
    ///
    /// let region = Region::new(ivec3(-1, -1, -1), ivec3(0, 0, 0));
    /// assert_eq!(1, region.volume());
    ///
    /// let region = Region::new(ivec3(0, 0, 0), ivec3(2, 2, 2));
    /// assert_eq!(8, region.volume());
    /// ```
    #[inline]
    #[must_use]
    pub fn volume(self) -> u64 {
        self.dimensions().as_u64vec3().element_product()
    }

    /// Returns `true` if the region contains the given value,
    /// inclusive of the maximum position.
    ///
    /// # Examples
    /// ```rust
    /// # use glam::{IVec3, ivec3};
    /// # use octo::Region;
    /// let region = Region::new(ivec3(-1, -1, -1), ivec3(3, 3, 3));
    ///
    /// assert!(region.contains(ivec3(-1, -1, -1)));
    /// assert!(region.contains(ivec3(3, 3, 3)));
    /// assert!(region.contains(ivec3(1, 1, 1)));
    ///
    /// // A region always contains itself inclusively.
    /// assert!(region.contains(region));
    ///
    /// let smaller = Region::new(ivec3(0, 0, 0), ivec3(2, 2, 2));
    /// assert!(region.contains(smaller));
    ///
    /// let bigger = Region::new(ivec3(-2, -2, -2), ivec3(4, 4, 4));
    /// assert!(!region.contains(bigger));
    ///
    /// let elsewhere = Region::new(ivec3(-10, -10, -10), ivec3(-9, -9, -9));
    /// assert!(!region.contains(elsewhere));
    /// ```
    #[inline]
    #[must_use]
    pub fn contains<T: RegionContained>(self, value: T) -> bool {
        value.contained(self)
    }

    /// Iterate over the positions in this region.
    #[inline]
    #[must_use]
    pub fn iter(self) -> impl Iterator<Item = IVec3> {
        itertools::iproduct!(
            self.min.x..=self.max.x,
            self.min.y..=self.max.y,
            self.min.z..=self.max.z
        )
        .map(IVec3::from)
    }

    // TODO: implement
    pub fn intersection(self, rhs: Self) -> Self {
        todo!()
    }
}

impl From<Range<IVec3>> for Region {
    fn from(value: Range<IVec3>) -> Self {
        Self::new(value.start, value.end)
    }
}

impl RegionContained for Region {
    fn contained(&self, region: Region) -> bool {
        region.contains(self.min()) && region.contains(self.max())
    }
}

/// Trait implemented by types that can be "contained" within a voxel region.
///
/// Implemented by vectors and other regions.
pub trait RegionContained {
    /// Test if the given region contains this value, INCLUSIVE of the maximum position of the region.
    fn contained(&self, region: Region) -> bool;
}

macro_rules! impl_region_bounded_vector {
    ($vec:ty) => {
        impl crate::RegionContained for $vec {
            #[inline]
            fn contained(&self, region: Region) -> bool {
                let [Ok(x), Ok(y), Ok(z)] = [
                    i32::try_from(self.x),
                    i32::try_from(self.y),
                    i32::try_from(self.z),
                ] else {
                    return false;
                };

                let pos = glam::ivec3(x, y, z);
                let min = region.min();
                let max = region.max();

                pos.cmple(max).all() && pos.cmpge(min).all()
            }
        }
    };
}

impl_region_bounded_vector!(glam::I16Vec3);
impl_region_bounded_vector!(glam::IVec3);
impl_region_bounded_vector!(glam::I64Vec3);
impl_region_bounded_vector!(glam::U16Vec3);
impl_region_bounded_vector!(glam::UVec3);
impl_region_bounded_vector!(glam::U64Vec3);
