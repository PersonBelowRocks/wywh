use bevy::math::IVec3;

use crate::data::tile::Face;

#[derive(
    Copy,
    Clone,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde::Deserialize,
    serde::Serialize,
    FromPrimitive,
    ToPrimitive,
)]
#[serde(rename_all = "snake_case")]
pub enum BlockModelFace {
    #[serde(alias = "u")]
    Up = 0,
    #[serde(alias = "d")]
    Down = 1,
    #[serde(alias = "l")]
    Left = 2,
    #[serde(alias = "r")]
    Right = 3,
    #[serde(alias = "f")]
    Front = 4,
    #[serde(alias = "b")]
    Back = 5,
}

impl BlockModelFace {
    pub fn to_usize(self) -> usize {
        use num_traits::ToPrimitive;
        ToPrimitive::to_usize(&self).unwrap()
    }
}

impl BlockModelFace {
    pub const FACES: [Self; 6] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::Front,
        Self::Back,
    ];
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockModelFaceMap<T>([Option<T>; 6]);

impl<T> BlockModelFaceMap<T> {
    pub fn filled(value: T) -> Self
    where
        T: Copy,
    {
        Self([Some(value); 6])
    }

    pub fn get(&self, face: BlockModelFace) -> Option<&T> {
        self.0[face.to_usize()].as_ref()
    }

    pub fn get_mut(&mut self, face: BlockModelFace) -> Option<&mut T> {
        self.0[face.to_usize()].as_mut()
    }

    pub fn set(&mut self, face: BlockModelFace, value: T) -> Option<T> {
        self.0[face.to_usize()].replace(value)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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

    pub fn get_cardinal_face(self, face: BlockModelFace) -> Face {
        match face {
            BlockModelFace::Up => self.up(),
            BlockModelFace::Down => self.down(),
            BlockModelFace::Left => self.left(),
            BlockModelFace::Right => self.right(),
            BlockModelFace::Front => self.front(),
            BlockModelFace::Back => self.back(),
        }
    }

    pub fn get_model_face(self, face: Face) -> BlockModelFace {
        match face {
            Face::Top => todo!(),
            Face::Bottom => todo!(),
            Face::North => todo!(),
            Face::East => todo!(),
            Face::South => todo!(),
            Face::West => todo!(),
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
