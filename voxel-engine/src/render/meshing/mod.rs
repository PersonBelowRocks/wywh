use std::array;

use bevy::math::{BVec3, IVec3};
use itertools::Itertools;

use crate::{
    data::tile::Face,
    topo::{
        access::{ChunkBounds, ReadAccess},
        chunk::ChunkPos,
        chunk_ref::ChunkVoxelOutput,
        realm::ChunkManager,
        storage::error::OutOfBounds,
    },
    util::FaceMap,
};

pub mod greedy;
pub mod immediate;

pub trait ChunkAccess: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}
impl<T> ChunkAccess for T where T: ReadAccess<ReadType = ChunkVoxelOutput> + ChunkBounds {}

// TODO: neighbors living on the corners
#[derive(Clone)]
pub struct Neighbors<C: ChunkAccess> {
    pos: ChunkPos,
    faces: FaceMap<C>,
    edges: [Option<C>; 12],
    corners: [Option<C>; 8],
}

#[derive(Copy, Clone, Debug)]
pub enum ExceedAt {
    Face,
    Edge,
    Corner,
}

#[derive(Copy, Clone, Debug)]
pub struct EdgeNeighbor {
    a: Face,
    b: Face,
}

#[derive(Copy, Clone, Debug)]
pub enum CornerNeighbor {
    NTE = 0,
    NTW = 1,
    NBE = 2,
    NBW = 3,
    STE = 4,
    STW = 5,
    SBE = 6,
    SBW = 7,
}

impl CornerNeighbor {
    #[inline]
    pub fn from_faces(a: Face, b: Face, c: Face) -> Option<Self> {
        /*
        the following must be true:
        a != b &&
        b != c &&
        a != c
        (they must be unique)
        */
        if a.axis() == b.axis() || a.axis() == c.axis() || a.axis() == c.axis() {
            return None;
        }

        let mut arr = [a, b, c];
        arr.sort_by(|q, p| q.axis().as_usize().cmp(&p.axis().as_usize()));

        Some(match arr {
            [Face::North, Face::Top, Face::East] => Self::NTE,
            [Face::North, Face::Top, Face::West] => Self::NTW,

            [Face::North, Face::Bottom, Face::East] => Self::NBE,
            [Face::North, Face::Bottom, Face::West] => Self::NBW,

            [Face::South, Face::Top, Face::East] => Self::STE,
            [Face::South, Face::Top, Face::West] => Self::STW,

            [Face::South, Face::Bottom, Face::East] => Self::SBE,
            [Face::South, Face::Bottom, Face::West] => Self::SBW,

            _ => unreachable!("all other conditions should have been eliminated by previous conditions and operations"),
        })
    }

    pub fn as_usize(self) -> usize {
        self as usize
    }
}

#[derive(Clone)]
pub struct NeighborsBuilder<C: ChunkAccess>(Neighbors<C>);

impl<C: ChunkAccess> NeighborsBuilder<C> {
    pub fn new(pos: ChunkPos) -> Self {
        Self(Neighbors {
            pos,
            faces: FaceMap::new(),
            edges: array::from_fn(|_| None),
            corners: array::from_fn(|_| None),
        })
    }

    pub fn what_neighbor(&self, pos: IVec3) -> Result<ExceedAt, OutOfBounds> {
        let cntr = IVec3::from(self.0.pos);

        let bv = cntr.cmplt(pos) & cntr.cmpgt(pos);
        let axes_exceeded: u32 = [bv.x, bv.y, bv.z].map(|b| b as u32).iter().sum();

        match axes_exceeded {
            0 => Err(OutOfBounds),
            1 => Ok(ExceedAt::Face),
            2 => Ok(ExceedAt::Edge),
            3 => Ok(ExceedAt::Corner),

            _ => unreachable!("we sum a 3d bool vector, so the max result is 3"),
        }
    }

    pub fn set_neighbor(&mut self, pos: IVec3, access: C) -> Result<(), OutOfBounds> {
        todo!()
    }
}
