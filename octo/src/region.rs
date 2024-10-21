use std::ops::Range;

use glam::{IVec3, UVec3};

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
    /// The region excludes the maximum position.
    #[inline]
    pub fn new(a: impl Into<IVec3>, b: impl Into<IVec3>) -> Self {
        let a: IVec3 = a.into();
        let b: IVec3 = b.into();

        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// Create a new region bounded by the 2 given positions.
    ///
    /// The region will include both of the given positions.
    ///
    /// # Panics
    /// Will panic if either of the bounding positions has a component of `i32::MAX`.
    #[inline]
    #[track_caller]
    pub fn new_inclusive(a: impl Into<IVec3>, b: impl Into<IVec3>) -> Self {
        let a: IVec3 = a.into();
        let b: IVec3 = b.into();

        let max = a.max(b);
        if max.cmpge(IVec3::MAX).any() {
            panic!(
                "Cannot create a region bounded inclusively by vectors with i32::MAX components"
            );
        }

        Self {
            min: a.min(b),
            max: max + IVec3::ONE,
        }
    }

    /// Returns `true` if this region is degenerate, i.e., it has no volume.
    #[inline]
    #[must_use]
    pub fn is_degenerate(self) -> bool {
        !self.dimensions().cmpgt(UVec3::ZERO).all()
    }

    /// The minimum position of the region.
    #[inline]
    #[must_use]
    pub fn min(self) -> IVec3 {
        self.min
    }

    /// The maximum position of the region. The region does not contain this position.
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

    /// Scale the region by some factor.
    ///
    /// # Examples
    /// ```rust
    /// # use octo::Region;
    /// # use glam::{IVec3, ivec3};
    ///
    /// let region = Region::new([0, 0, 0], [1, 1, 1]);
    /// assert_eq!(ivec3(0, 0, 0), region.scaled(16).min());
    /// assert_eq!(ivec3(16, 16, 16), region.scaled(16).max());
    ///
    /// let region = Region::new([-1, -1, -1], [1, 2, 1]);
    /// assert_eq!(ivec3(-16, -16, -16), region.scaled(16).min());
    /// assert_eq!(ivec3(16, 32, 16), region.scaled(16).max());
    /// ```
    #[inline]
    #[must_use]
    pub fn scaled(self, scale: i32) -> Region {
        Region::new(self.min() * scale, self.max() * scale)
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

    /// Returns `true` if the region contains the given value, exclusive of the maximum position.
    ///
    /// # Examples
    /// ```rust
    /// # use glam::{IVec3, ivec3};
    /// # use octo::Region;
    /// let region = Region::new(ivec3(-1, -1, -1), ivec3(3, 3, 3));
    ///
    /// assert!(region.contains(ivec3(-1, -1, -1)));
    /// assert!(region.contains(ivec3(1, 1, 1)));
    /// // These are on the maximum border(s) of the region, so they're not contained.
    /// assert!(!region.contains(ivec3(3, 3, 3)));
    /// assert!(!region.contains(ivec3(0, 3, 0)));
    ///
    /// // A region always contains itself.
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

    /// Iterate over all the positions contained within this region.
    #[inline]
    #[must_use]
    pub fn iter(self) -> impl Iterator<Item = IVec3> {
        itertools::iproduct!(
            self.min.x..self.max.x,
            self.min.y..self.max.y,
            self.min.z..self.max.z
        )
        .map(IVec3::from)
    }

    /// Find the region defined by the intersection of 2 other regions.
    /// Returns `None` if the regions do not intersect.
    ///
    /// # Examples
    ///
    /// Running `intersection()` for two regions that actually intersect:
    /// ```rust
    /// # use glam::{IVec3, ivec3};
    /// # use octo::Region;
    ///
    /// let region_1 = Region::new([-8, -8, -8], [7, 7, 7]);
    /// let region_2 = Region::new([-8, -10, -8], [-6, -6, -6]);
    ///
    /// let intersection = region_1.intersection(region_2).unwrap();
    ///
    /// assert_eq!(ivec3(-8, -8, -8), intersection.min());
    /// assert_eq!(ivec3(-6, -6, -6), intersection.max());
    /// ```
    ///
    /// Running `intersection()` for two regions that do not intersect:
    /// ```rust
    /// # use glam::{IVec3, ivec3};
    /// # use octo::Region;
    ///
    /// let region_1 = Region::new([0, 0, 0], [6, 6, 6]);
    /// let region_2 = Region::new([-2, -2, -2], [0, 0, 0]);
    ///
    /// let maybe_intersection = region_1.intersection(region_2);
    /// assert!(maybe_intersection.is_none());
    /// ```
    #[inline]
    #[must_use]
    pub fn intersection(self, rhs: Self) -> Option<Self> {
        if !self.overlaps(rhs) {
            return None;
        }

        let min = IVec3::max(self.min(), rhs.min());
        let max = IVec3::min(self.max(), rhs.max());

        Some(Self::new(min, max))
    }

    // TODO: doctest degenerate intersection
    /// Returns `true` if the two regions overlap.
    ///
    /// # Examples
    /// ```rust
    /// # use octo::Region;
    /// # use glam::{ivec3, IVec3};
    ///
    /// let region = Region::new([-4, -4, -4], [4, 4, 4]);
    /// assert!(region.overlaps(Region::new([0, 0, 0], [2, 2, 2])));
    /// // These regions overlap, but one is not a subregion of the other.
    /// assert!(region.overlaps(Region::new([-16, -16, -16], [0, 0, 0])));
    /// // These regions are touching but not overlapping.
    /// assert!(!region.overlaps(Region::new([-16, -16, -16], [-4, -4, -4])));
    /// // These regions are nowhere near eachother
    /// assert!(!region.overlaps(Region::new([100, 100, 100], [120, 120, 120])));
    /// ```
    #[inline]
    #[must_use]
    pub fn overlaps(self, rhs: Self) -> bool {
        self.min().cmplt(rhs.max()).all() && self.max().cmpge(rhs.min()).all()
    }
}

impl From<Range<IVec3>> for Region {
    fn from(value: Range<IVec3>) -> Self {
        Self::new(value.start, value.end)
    }
}

impl RegionContained for Region {
    fn contained(&self, region: Region) -> bool {
        // Need to subtract one at the end here because the max position isn't even contained in its own region,
        // but is rather the exclusive upper bound of the region.
        region.contains(self.min()) && region.contains(self.max() - IVec3::ONE)
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

                pos.cmplt(max).all() && pos.cmpge(min).all()
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
