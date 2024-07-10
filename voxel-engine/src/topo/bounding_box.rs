use std::any::type_name;

use bevy::prelude::*;
use bevy::render::primitives::Aabb;

use crate::util::CartesianIterator3d;

use super::world::Chunk;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct BoundingBox {
    pub(crate) min: IVec3,
    pub(crate) max: IVec3,
}

impl BoundingBox {
    /// Panics if any component in `min` is greater than or equal to that component in `max`
    pub fn from_min_max(min: IVec3, max: IVec3) -> Self {
        if min.cmpge(max).any() {
            panic!(
                "Tried to create {} invalid min/max vectors",
                type_name::<Self>()
            )
        }

        Self { min, max }
    }

    pub fn new(a: IVec3, b: IVec3) -> Self {
        Self::from_min_max(a.min(b), a.max(b))
    }

    pub fn min(self) -> IVec3 {
        self.min
    }

    pub fn max(self) -> IVec3 {
        self.max
    }

    pub fn is_chunk(self) -> bool {
        self == Chunk::BOUNDING_BOX
    }

    pub fn contains(self, pos: IVec3) -> bool {
        pos.cmpge(self.min).all() && pos.cmplt(self.max).all()
    }

    pub fn contains_inclusive(self, pos: IVec3) -> bool {
        pos.cmpge(self.min).all() && pos.cmple(self.max).all()
    }

    pub fn to_aabb(self) -> Aabb {
        Aabb::from_min_max(self.min.as_vec3(), self.max.as_vec3())
    }

    pub fn span(self) -> Self {
        Self {
            min: IVec3::splat(0),
            max: (self.max - self.min).abs(),
        }
    }

    pub fn volume(self) -> u32 {
        let [x, y, z] = self.span().max.to_array();
        (x * y * z).unsigned_abs()
    }

    pub fn cartesian_iter(self) -> CartesianIterator3d {
        CartesianIterator3d::new(self.min, self.max).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_box_contains() {
        let bb = Chunk::BOUNDING_BOX;

        assert!(bb.contains(IVec3::splat(0)));
        assert!(bb.contains(IVec3::splat(15)));

        assert!(!bb.contains(IVec3::splat(16)));
        assert!(bb.contains_inclusive(IVec3::splat(16)));
    }
}
