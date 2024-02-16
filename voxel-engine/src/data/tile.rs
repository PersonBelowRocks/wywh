use bevy::prelude::*;

use crate::util::Axis3D;

use super::error::FaceParseError;

#[derive(
    Copy,
    Clone,
    Default,
    Debug,
    Hash,
    PartialEq,
    Eq,
    dm::Display,
    serde::Deserialize,
    serde::Serialize,
)]
pub enum Transparency {
    #[default]
    #[serde(rename = "opaque")]
    Opaque,
    #[serde(rename = "trans")]
    Transparent,
}

impl Transparency {
    pub fn is_opaque(self) -> bool {
        matches!(self, Self::Opaque)
    }

    pub fn is_transparent(self) -> bool {
        matches!(self, Self::Transparent)
    }
}

/// Faces of a cube
#[allow(dead_code)]
#[derive(
    FromPrimitive,
    ToPrimitive,
    PartialEq,
    Eq,
    Hash,
    Debug,
    Copy,
    Clone,
    serde::Deserialize,
    serde::Serialize,
)]
pub enum Face {
    #[serde(rename(serialize = "t"))]
    #[serde(alias = "top")]
    #[serde(alias = "t")]
    Top = 0,

    #[serde(rename(serialize = "b"))]
    #[serde(alias = "bottom")]
    #[serde(alias = "b")]
    Bottom = 1,

    #[serde(rename(serialize = "n"))]
    #[serde(alias = "north")]
    #[serde(alias = "n")]
    North = 2,

    #[serde(rename(serialize = "e"))]
    #[serde(alias = "east")]
    #[serde(alias = "e")]
    East = 3,

    #[serde(rename(serialize = "s"))]
    #[serde(alias = "south")]
    #[serde(alias = "s")]
    South = 4,

    #[serde(rename(serialize = "w"))]
    #[serde(alias = "west")]
    #[serde(alias = "w")]
    West = 5,
}

impl std::str::FromStr for Face {
    type Err = FaceParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "t" | "top" => Self::Top,
            "b" | "bottom" => Self::Bottom,
            "n" | "north" => Self::North,
            "e" | "east" => Self::East,
            "s" | "south" => Self::South,
            "w" | "west" => Self::West,

            _ => return Err(Self::Err::new(s.to_string())),
        })
    }
}

impl Face {
    /// Array of all (6) voxel faces.
    /// Useful for iterating through to apply an operation to each face.
    pub const FACES: [Face; 6] = [
        Face::Top,
        Face::Bottom,
        Face::North,
        Face::East,
        Face::South,
        Face::West,
    ];

    /// Offset the given [`pos`] by 1 in the direction of the face.
    /// Say `V` is some voxel, and we want to get the position of the voxel
    /// 1 step east of `V`. We can use this function to do just that through
    /// `Face::East.get_position_offset(position of V)`.
    #[inline]
    pub fn offset_position(self, pos: IVec3) -> IVec3 {
        pos + self.normal()
    }

    #[inline]
    pub fn as_usize(self) -> usize {
        num_traits::ToPrimitive::to_usize(&self).unwrap()
    }

    #[inline]
    pub fn as_u8(self) -> u8 {
        self.as_usize() as u8
    }

    #[inline]
    pub fn as_u32(self) -> u32 {
        self.as_usize() as u32
    }

    #[inline]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
        }
    }

    #[inline]
    pub fn normal(self) -> IVec3 {
        match self {
            Face::Top => [0, 1, 0],
            Face::Bottom => [0, -1, 0],
            Face::North => [1, 0, 0],
            Face::East => [0, 0, 1],
            Face::South => [-1, 0, 0],
            Face::West => [0, 0, -1],
        }
        .into()
    }

    #[inline]
    pub fn axis_direction(self) -> i32 {
        match self {
            Face::Top | Face::North | Face::East => 1,
            Face::Bottom | Face::South | Face::West => -1,
        }
    }

    // TODO: this should be reworked or removed
    #[inline]
    pub fn pos_on_face(self, pos: IVec3) -> IVec2 {
        match self {
            Face::Top => [pos.x, pos.z],
            Face::Bottom => [pos.x, pos.z],
            Face::North => [pos.y, pos.z],
            Face::East => [pos.x, pos.y],
            Face::South => [pos.y, pos.z],
            Face::West => [pos.x, pos.y],
        }
        .into()
    }

    #[inline]
    pub const fn axis(self) -> Axis3D {
        match self {
            Face::North | Face::South => Axis3D::X,
            Face::Top | Face::Bottom => Axis3D::Y,
            Face::East | Face::West => Axis3D::Z,
        }
    }

    #[inline]
    pub const fn from_normal(normal: IVec3) -> Option<Self> {
        match normal.to_array() {
            [0, 1, 0] => Some(Self::Top),
            [0, -1, 0] => Some(Self::Bottom),
            [1, 0, 0] => Some(Self::North),
            [0, 0, 1] => Some(Self::East),
            [-1, 0, 0] => Some(Self::South),
            [0, 0, -1] => Some(Self::West),
            _ => None,
        }
    }

    #[inline]
    pub fn rotation_between(self, target: Self) -> Quat {
        Quat::from_rotation_arc(self.normal().as_vec3(), target.normal().as_vec3())
    }

    #[inline]
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::North | Self::East | Self::South | Self::West)
    }

    #[inline]
    pub const fn is_vertical(self) -> bool {
        !self.is_horizontal()
    }

    #[inline]
    pub fn is_orthogonal(self, other: Self) -> bool {
        self.normal().dot(other.normal()) == 0
    }
}
