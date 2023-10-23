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
    pub fn offset_position(self, pos: IVec3) -> IVec3 {
        pos + self.normal()
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
    pub fn axis(self) -> Axis3D {
        match self {
            Face::North | Face::South => Axis3D::X,
            Face::Top | Face::Bottom => Axis3D::Y,
            Face::East | Face::West => Axis3D::Z,
        }
    }

    #[inline]
    pub fn rotation_between(self, target: Self) -> Quat {
        Quat::from_rotation_arc(self.normal().as_vec3(), target.normal().as_vec3())
    }
}

#[derive(dm::Into, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Copy, Clone)]
pub struct VoxelId(u32);

impl VoxelId {
    // TODO: better name for this, air implies this voxel is made of "air" which might communicate the wrong idea, maybe void?
    // this special VoxelId is used to represent a position which is ready to be occupied, but currently
    // isn't
    pub const AIR: Self = VoxelId::new(0);

    // TODO: is the name here okay? and is u32::MAX a good internal value?
    // this special VoxelId communicates that the value in this position *does not matter*.
    // this is quite different from VoxelId::AIR because that represents that a space is empty and can be occupied whereas
    // VoxelId::IGNORE doesn't have to be empty, we should just ignore it.
    // such an ID is *extremely* useful in for example buffered access operations, you can initialize
    // an internal voxel buffer filled with VoxelId::IGNORE and when a voxel is inserted, to overwrite that ID with
    // whatever got inserted. then, upon flushing the buffer into whatever underlying access you wanted to edit, you
    // just copy over everything that *isn't* a VoxelId::IGNORE  so that for these positions the underlying access retains
    // the previous data that was there. basically, every position with VoxelId::IGNORE in it was unedited.
    pub const IGNORE: Self = VoxelId::new(u32::MAX);

    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn debug_transparency(self) -> Transparency {
        match self.0 {
            0 => Transparency::Transparent,
            _ => Transparency::Opaque,
        }
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
