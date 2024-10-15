/// Iterate over a 3d grid with the given dimensions.
/// Iteration order is x < y < z, where z is the innermost "loop".
///
/// # Panics
/// Panics if any of the ranges start at a value that is greater than or equal to their ending value.
/// For example:
/// - `cartesian_grid!(0..0, 0..4, 0..4)` would panic because the range `0..0` is invalid.
/// - `cartesian_grid!(0..4, 6..4, 0..4)` would panic because the range `6..4` is invalid.
/// - `cartesian_grid!(0..4, 0..4, 4..4)` would panic because the range `4..4` is invalid.
#[macro_export]
macro_rules! cartesian_grid {
    ($p_range:expr) => {{
        use std::ops::RangeBounds;

        let start = match $p_range.start_bound() {
            std::ops::Bound::Included(&v) => v,
            std::ops::Bound::Excluded(&v) => v.saturating_add(bevy::math::IVec3::ONE),
            std::ops::Bound::Unbounded => bevy::prelude::IVec3::splat(i32::MIN),
        };

        let end = match $p_range.end_bound() {
            std::ops::Bound::Included(&v) => v,
            std::ops::Bound::Excluded(&v) => v.saturating_sub(bevy::math::IVec3::ONE),
            std::ops::Bound::Unbounded => bevy::prelude::IVec3::splat(i32::MAX),
        };

        cartesian_grid!(start.x..=end.x, start.y..=end.y, start.z..=end.z)
    }};

    ($x:expr, $y:expr, $z:expr) => {{
        #[doc(hidden)]
        fn _inclusive_range<R>(range: R) -> std::ops::RangeInclusive<i32>
        where
            R: std::ops::RangeBounds<i32> + std::fmt::Debug,
        {
            let start = match range.start_bound() {
                std::ops::Bound::Included(&v) => v,
                std::ops::Bound::Excluded(&v) => v.saturating_add(1),
                std::ops::Bound::Unbounded => i32::MIN,
            };

            let end = match range.end_bound() {
                std::ops::Bound::Included(&v) => v,
                std::ops::Bound::Excluded(&v) => v.saturating_sub(1),
                std::ops::Bound::Unbounded => i32::MAX,
            };

            assert!(
                start < end,
                "Range {range:?} must start with a value less than its end"
            );

            start..=end
        }

        let rx: std::ops::RangeInclusive<i32> = _inclusive_range($x);
        let ry: std::ops::RangeInclusive<i32> = _inclusive_range($y);
        let rz: std::ops::RangeInclusive<i32> = _inclusive_range($z);

        itertools::iproduct!(rx, ry, rz)
            .map(<[i32; 3]>::from)
            .map(IVec3::from_array)
    }};
}

#[cfg(test)]
mod tests {
    use bevy::math::{ivec3, IVec3};

    #[test]
    fn iteration_order_vector() {
        let mut iterator = cartesian_grid!(ivec3(-5, -3, -5)..=ivec3(5, 6, 5));

        assert_eq!(Some(ivec3(-5, -3, -5)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -4)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -3)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -2)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -1)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 0)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 1)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 2)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 3)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 4)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 5)), iterator.next());
        assert_eq!(Some(ivec3(-5, -2, -5)), iterator.next());
    }

    #[test]
    fn iteration_order() {
        let mut iterator = cartesian_grid!(-5..=5, -3..=6, -5..=5);

        assert_eq!(Some(ivec3(-5, -3, -5)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -4)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -3)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -2)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, -1)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 0)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 1)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 2)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 3)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 4)), iterator.next());
        assert_eq!(Some(ivec3(-5, -3, 5)), iterator.next());
        assert_eq!(Some(ivec3(-5, -2, -5)), iterator.next());

        let mut smaller = cartesian_grid!(-1..=1, -1..=1, -1..=1);
        // X = -1
        assert_eq!(Some(ivec3(-1, -1, -1)), smaller.next());
        assert_eq!(Some(ivec3(-1, -1, 0)), smaller.next());
        assert_eq!(Some(ivec3(-1, -1, 1)), smaller.next());
        assert_eq!(Some(ivec3(-1, 0, -1)), smaller.next());
        assert_eq!(Some(ivec3(-1, 0, 0)), smaller.next());
        assert_eq!(Some(ivec3(-1, 0, 1)), smaller.next());
        assert_eq!(Some(ivec3(-1, 1, -1)), smaller.next());
        assert_eq!(Some(ivec3(-1, 1, 0)), smaller.next());
        assert_eq!(Some(ivec3(-1, 1, 1)), smaller.next());

        // X = 0
        assert_eq!(Some(ivec3(0, -1, -1)), smaller.next());
        assert_eq!(Some(ivec3(0, -1, 0)), smaller.next());
        assert_eq!(Some(ivec3(0, -1, 1)), smaller.next());
        assert_eq!(Some(ivec3(0, 0, -1)), smaller.next());
        assert_eq!(Some(ivec3(0, 0, 0)), smaller.next());
        assert_eq!(Some(ivec3(0, 0, 1)), smaller.next());
        assert_eq!(Some(ivec3(0, 1, -1)), smaller.next());
        assert_eq!(Some(ivec3(0, 1, 0)), smaller.next());
        assert_eq!(Some(ivec3(0, 1, 1)), smaller.next());

        // X = 1
        assert_eq!(Some(ivec3(1, -1, -1)), smaller.next());
        assert_eq!(Some(ivec3(1, -1, 0)), smaller.next());
        assert_eq!(Some(ivec3(1, -1, 1)), smaller.next());
        assert_eq!(Some(ivec3(1, 0, -1)), smaller.next());
        assert_eq!(Some(ivec3(1, 0, 0)), smaller.next());
        assert_eq!(Some(ivec3(1, 0, 1)), smaller.next());
        assert_eq!(Some(ivec3(1, 1, -1)), smaller.next());
        assert_eq!(Some(ivec3(1, 1, 0)), smaller.next());
        assert_eq!(Some(ivec3(1, 1, 1)), smaller.next());

        // Done
        assert_eq!(None, smaller.next());
    }

    #[test]
    fn size_hint() {
        let iterator = cartesian_grid!(-1..=1, -1..=1, -1..=1);
        let size = 3 * 3 * 3;
        assert_eq!((size, Some(size)), iterator.size_hint());
    }

    #[test]
    #[should_panic]
    fn equal_start_and_end() {
        let _ = cartesian_grid!(0..2, -1..2, -1..-1);
    }

    #[test]
    #[should_panic]
    fn greater_start() {
        let _ = cartesian_grid!(0..2, 5..2, 0..4);
    }
}
