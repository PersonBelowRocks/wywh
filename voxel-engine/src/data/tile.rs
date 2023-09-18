use bevy::prelude::*;

use crate::util::Axis3D;

use super::error::TileDataConversionError;

#[derive(Copy, Clone, Default, Debug, Hash, PartialEq, Eq, dm::Display)]
pub enum Transparency {
    #[default]
    Opaque,
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
#[derive(FromPrimitive, ToPrimitive, PartialEq, Eq, Hash, Debug, Copy, Clone)]
pub enum Face {
    Top = 0,
    Bottom = 1,
    North = 2,
    East = 3,
    South = 4,
    West = 5,
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
    pub fn offset_position(&self, pos: IVec3) -> IVec3 {
        pos + self.offset()
    }

    #[inline]
    pub fn opposite(self) -> Self {
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
    pub fn offset(self) -> IVec3 {
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
}

#[derive(dm::Into, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Copy, Clone)]
pub struct VoxelId(u32);

impl VoxelId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl From<u32> for VoxelId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TextureId(pub(crate) u32);

#[derive(PartialEq, Eq)]
pub struct TileData {
    voxel_id: VoxelId,
    texture_id: TextureId,
}

enum TextureType {
    Mono(Handle<Image>),
    Multi {
        default: Handle<Image>,
        faces: hb::HashMap<Face, Handle<Image>>,
    },
}

pub struct VoxelTexture {
    texture: TextureType,
}

pub trait AsTile: Sized {
    fn to_tile_data(&self) -> Result<TileData, TileDataConversionError>;
    fn from_tile_data(data: &TileData) -> Result<Self, TileDataConversionError>;
}
