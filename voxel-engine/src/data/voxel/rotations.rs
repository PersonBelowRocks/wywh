use bevy::math::IVec3;

use crate::data::tile::Face;

#[derive(Copy, Clone, Debug)]
pub enum BlockModelFace {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
    Front = 4,
    Back = 5,
}

#[derive(Copy, Clone, Debug)]
pub struct BlockModelRotation {
    fwd: Face,
    up: Face,
}

impl BlockModelRotation {
    pub fn new(fwd: Face, up: Face) -> Option<Self> {
        if fwd.is_orthogonal(up) {
            Some(Self { fwd, up })
        } else {
            None
        }
    }

    pub fn cross_product(self) -> IVec3 {
        self.front().normal().cross(self.up().normal())
    }

    pub fn right(self) -> Face {
        Face::from_normal(self.cross_product()).unwrap()
    }

    pub fn left(self) -> Face {
        Face::from_normal(-self.cross_product()).unwrap()
    }

    pub fn up(self) -> Face {
        self.up
    }

    pub fn down(self) -> Face {
        Face::from_normal(-self.up().normal()).unwrap()
    }

    pub fn front(self) -> Face {
        self.fwd
    }

    pub fn back(self) -> Face {
        Face::from_normal(-self.front().normal()).unwrap()
    }

    pub fn get(self, face: BlockModelFace) -> Face {
        match face {
            BlockModelFace::Up => self.up(),
            BlockModelFace::Down => self.down(),
            BlockModelFace::Left => self.left(),
            BlockModelFace::Right => self.right(),
            BlockModelFace::Front => self.front(),
            BlockModelFace::Back => self.back(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correct_handedness() {
        let rot = BlockModelRotation::new(Face::North, Face::Top).unwrap();

        assert_eq!(Face::West, rot.left());
        assert_eq!(Face::East, rot.right());
    }

    #[test]
    fn test_basic_rotations() {
        // we're upside-down and looking west
        let rot = BlockModelRotation::new(Face::West, Face::Bottom).unwrap();

        assert_eq!(Face::Bottom, rot.up());
        assert_eq!(Face::Top, rot.down());

        // we're upside down so these are flipped!
        assert_eq!(Face::North, rot.left());
        assert_eq!(Face::South, rot.right());

        // this one isnt upside down so it's not flipped
        let rot2 = BlockModelRotation::new(Face::West, Face::Top).unwrap();
        assert_eq!(Face::North, rot2.right());
        assert_eq!(Face::South, rot2.left());

        assert_eq!(Face::West, rot.front());
        assert_eq!(Face::East, rot.back());
    }
}
