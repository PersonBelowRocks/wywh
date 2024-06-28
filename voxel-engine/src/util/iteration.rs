use std::ops::Range;

use bevy::math::{ivec3, IVec3};

#[derive(Debug, PartialEq, Eq)]
pub struct CartesianIterator3d {
    x: Range<i32>,
    cur_x: i32,
    y: Range<i32>,
    cur_y: i32,
    z: Range<i32>,
    cur_z: i32,
}

impl CartesianIterator3d {
    pub fn new(p1: IVec3, p2: IVec3) -> Option<Self> {
        let min = p1.min(p2);
        let max = p1.max(p2);

        if (max - min).cmple(IVec3::ZERO).any() {
            None
        } else {
            Some(Self {
                x: min.x..max.x,
                y: min.y..max.y,
                z: min.z..max.z,

                cur_x: min.x,
                cur_y: min.y,
                cur_z: min.z,
            })
        }
    }
}

impl Iterator for CartesianIterator3d {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        let out = if self.cur_x > self.x.end {
            None
        } else {
            Some(ivec3(self.cur_x, self.cur_y, self.cur_z))
        };

        self.cur_z += 1;
        if self.cur_z > self.z.end {
            self.cur_z = self.z.start;
            self.cur_y += 1;

            if self.cur_y > self.y.end {
                self.cur_y = self.y.start;
                self.cur_x += 1;
            }
        }

        out
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let bx = self.x.end - self.x.start;
        let by = self.y.end - self.y.start;
        let bz = self.z.end - self.z.start;

        let vol = bx * by * bz;

        (vol as usize, Some(vol as usize))
    }
}

impl ExactSizeIterator for CartesianIterator3d {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iteration_order() {
        let mut iterator = CartesianIterator3d::new(ivec3(-5, -3, -5), ivec3(5, 6, 5)).unwrap();

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

        let mut smaller = CartesianIterator3d::new(ivec3(-1, -1, -1), ivec3(1, 1, 1)).unwrap();
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
        let iterator = CartesianIterator3d::new(ivec3(-1, -1, -1), ivec3(1, 1, 1)).unwrap();
        let size = 3 * 3 * 3;
        assert_eq!((size, Some(size)), iterator.size_hint());
    }

    #[test]
    fn invalid_dimensions() {
        let iterator = CartesianIterator3d::new(ivec3(0, -1, -1), ivec3(2, 2, -1));
        assert!(iterator.is_none());
    }
}
