use bevy::prelude::*;

use super::chunk::Chunk;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct BoundingBox {
    pub min: IVec3,
    pub max: IVec3,
}

impl BoundingBox {
    pub fn is_chunk(self) -> bool {
        self == Chunk::BOUNDING_BOX
    }

    pub fn contains(self, pos: IVec3) -> bool {
        pos.cmpge(self.min).all() && pos.cmplt(self.max).all()
    }

    pub fn contains_inclusive(self, pos: IVec3) -> bool {
        pos.cmpge(self.min).all() && pos.cmple(self.max).all()
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
