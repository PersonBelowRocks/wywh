use std::marker::PhantomData;

use bevy::math::{ivec3, IVec2, IVec3};

use crate::{
    data::tile::Face,
    topo::{bounding_box::BoundingBox, ivec_project_to_3d},
    util::ivec3_to_1d,
};

use super::{
    block::BlockVoxel,
    error::NeighborAccessError,
    world::{Chunk, OutOfBounds},
};

fn localspace_to_chunk_pos(pos: IVec3) -> IVec3 {
    ivec3(
        pos.x.div_euclid(Chunk::SIZE),
        pos.y.div_euclid(Chunk::SIZE),
        pos.z.div_euclid(Chunk::SIZE),
    )
}

fn localspace_to_neighbor_localspace(pos: IVec3) -> IVec3 {
    ivec3(
        pos.x.rem_euclid(Chunk::SIZE),
        pos.y.rem_euclid(Chunk::SIZE),
        pos.z.rem_euclid(Chunk::SIZE),
    )
}

// TODO: document what localspace, worldspace, chunkspace, and facespace are
pub struct Neighbors<'a> {
    // TODO: type
    chunks: [Option<()>; NEIGHBOR_ARRAY_SIZE],
    default: BlockVoxel,
    _lt: PhantomData<&'a ()>,
}

/// Test if the provided facespace vector is in bounds
pub fn is_in_bounds(pos: IVec2) -> bool {
    let min: IVec2 = -IVec2::ONE;
    let max: IVec2 = IVec2::splat(Chunk::SIZE) + IVec2::ONE;

    pos.cmpge(min).all() && pos.cmplt(max).all()
}

/// Test if the provided localspace vector is in bounds
pub fn is_in_bounds_3d(pos: IVec3) -> bool {
    let min: IVec3 = -IVec3::ONE;
    let max: IVec3 = IVec3::splat(Chunk::SIZE) + IVec3::ONE;

    pos.cmpge(min).all() && pos.cmplt(max).all() && localspace_to_chunk_pos(pos) != IVec3::ZERO
}

// TODO: type
pub type NbResult<'a> = Result<(), NeighborAccessError>;

pub const NEIGHBOR_CUBIC_ARRAY_DIMENSIONS: usize = 3;
pub const NEIGHBOR_ARRAY_SIZE: usize = NEIGHBOR_CUBIC_ARRAY_DIMENSIONS.pow(3);

impl<'a> Neighbors<'a> {
    pub fn from_raw(chunks: [Option<()>; NEIGHBOR_ARRAY_SIZE], default: BlockVoxel) -> Self {
        Self {
            chunks,
            default,
            _lt: PhantomData,
        }
    }

    /// `pos` is in localspace
    fn internal_get(&self, pos: IVec3) -> NbResult<'_> {
        let chk_pos = localspace_to_chunk_pos(pos);

        if chk_pos == IVec3::ZERO {
            // tried to access center chunk (aka. the chunk for which we represent the neighbors)
            return Err(NeighborAccessError::OutOfBounds);
        }

        let chk_index = ivec3_to_1d(chk_pos + IVec3::ONE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS)
            .map_err(|_| NeighborAccessError::OutOfBounds)?;
        let chk = self
            .chunks
            .get(chk_index)
            .ok_or(NeighborAccessError::OutOfBounds)?;

        match chk {
            Some(access) => {
                let neighbor_local = localspace_to_neighbor_localspace(pos);
                todo!()
            }
            None => todo!(),
        }
    }

    /// `pos` in facespace
    pub fn get(&self, face: Face, pos: IVec2) -> NbResult<'_> {
        if !is_in_bounds(pos) {
            return Err(NeighborAccessError::OutOfBounds);
        }

        let pos_3d = {
            let mut mag = face.axis_direction();
            if mag > 0 {
                mag = Chunk::SIZE;
            }

            ivec_project_to_3d(pos, face, mag)
        };

        self.internal_get(pos_3d)
    }

    /// `pos` in localspace
    pub fn get_3d(&self, pos: IVec3) -> NbResult<'_> {
        if !is_in_bounds_3d(pos) {
            return Err(NeighborAccessError::OutOfBounds);
        }

        self.internal_get(pos)
    }
}

fn is_valid_neighbor_chunk_pos(pos: IVec3) -> bool {
    const BB: BoundingBox = BoundingBox {
        min: IVec3::splat(-1),
        max: IVec3::ONE,
    };

    pos != IVec3::ZERO && BB.contains_inclusive(pos)
}

pub struct NeighborsBuilder<'a>(Neighbors<'a>);

impl<'a> NeighborsBuilder<'a> {
    pub fn new(default: BlockVoxel) -> Self {
        Self(Neighbors::from_raw(Default::default(), default))
    }

    pub fn set_neighbor(&mut self, pos: IVec3, access: ()) -> Result<(), OutOfBounds> {
        if !is_valid_neighbor_chunk_pos(pos) {
            return Err(OutOfBounds);
        }

        let idx = ivec3_to_1d(pos + IVec3::ONE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS)
            .map_err(|_| OutOfBounds)?;

        let slot = self.0.chunks.get_mut(idx).ok_or(OutOfBounds)?;
        *slot = Some(access);

        Ok(())
    }

    pub fn build(self) -> Neighbors<'a> {
        self.0
    }
}
