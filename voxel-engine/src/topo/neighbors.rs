use bevy::math::{ivec3, IVec2, IVec3};

use crate::{
    data::{registries::block::BlockVariantId, tile::Face},
    topo::{bounding_box::BoundingBox, ivec_project_to_3d},
    util::ivec3_to_1d,
};

use super::{
    error::NeighborReadError,
    world::{chunk::ChunkReadHandle, Chunk, OutOfBounds},
};

fn localspace_to_chunk_pos(pos: IVec3) -> IVec3 {
    // TODO: use bitwise math
    ivec3(
        pos.x.div_euclid(Chunk::SIZE),
        pos.y.div_euclid(Chunk::SIZE),
        pos.z.div_euclid(Chunk::SIZE),
    )
}

fn localspace_to_neighbor_localspace(pos: IVec3) -> IVec3 {
    // TODO: use bitwise math
    ivec3(
        pos.x.rem_euclid(Chunk::SIZE),
        pos.y.rem_euclid(Chunk::SIZE),
        pos.z.rem_euclid(Chunk::SIZE),
    )
}

// TODO: document what localspace, worldspace, chunkspace, and facespace are
pub struct Neighbors<'a> {
    chunks: [Option<ChunkReadHandle<'a>>; NEIGHBOR_ARRAY_SIZE],
    default_block: BlockVariantId,
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

pub type NbResult<'a> = Result<BlockVariantId, NeighborReadError>;

pub const NEIGHBOR_CUBIC_ARRAY_DIMENSIONS: usize = 3;
pub const NEIGHBOR_ARRAY_SIZE: usize = NEIGHBOR_CUBIC_ARRAY_DIMENSIONS.pow(3);

impl<'a> Neighbors<'a> {
    pub fn from_raw(
        chunks: [Option<ChunkReadHandle<'a>>; NEIGHBOR_ARRAY_SIZE],
        default_block: BlockVariantId,
    ) -> Self {
        Self {
            chunks,
            default_block,
        }
    }

    /// Get a block in one of the neighboring chunks, returning the default block if there was no handle
    /// for that chunk. This function allows reading from all blocks in the neighboring chunks, not just
    /// the ones on the borders facing the center.
    /// # Vectors
    /// `ls_nb_pos` is in neighbor-only localspace
    pub fn get_3d(&self, ls_nb_pos: IVec3) -> NbResult<'_> {
        let chk_pos = localspace_to_chunk_pos(ls_nb_pos);

        if chk_pos == IVec3::ZERO {
            // tried to access center chunk (aka. the chunk for which we represent the neighbors)
            return Err(NeighborReadError::OutOfBounds);
        }

        let chk_index = ivec3_to_1d(chk_pos + IVec3::ONE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS)
            .map_err(|_| NeighborReadError::OutOfBounds)?;
        let chk = self
            .chunks
            .get(chk_index)
            .ok_or(NeighborReadError::OutOfBounds)?;

        match chk {
            Some(handle) => {
                let neighbor_local = localspace_to_neighbor_localspace(ls_nb_pos);
                Ok(handle.get(neighbor_local)?)
            }
            // No handle at this position so we return our default block
            None => Ok(self.default_block),
        }
    }

    /// Get the block in a neighboring chunk that "obscures" the given block position in the center chunk.
    /// The position may exceed the chunks borders by 1 to allow getting blocks diagonal of the center chunk.
    ///
    /// # Vectors
    /// `face_pos` is in local-facespace
    pub fn get(&self, face: Face, face_pos: IVec2) -> NbResult<'_> {
        if !is_in_bounds(face_pos) {
            return Err(NeighborReadError::OutOfBounds);
        }

        let pos_3d = {
            let mut mag = face.axis_direction();
            if mag > 0 {
                mag = Chunk::SIZE;
            }

            ivec_project_to_3d(face_pos, face, mag)
        };

        self.get_3d(pos_3d)
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
    pub fn new(default_block: BlockVariantId) -> Self {
        Self(Neighbors::from_raw(Default::default(), default_block))
    }

    pub fn set_neighbor(
        &mut self,
        pos: IVec3,
        handle: ChunkReadHandle<'a>,
    ) -> Result<(), OutOfBounds> {
        if !is_valid_neighbor_chunk_pos(pos) {
            return Err(OutOfBounds);
        }

        let idx = ivec3_to_1d(pos + IVec3::ONE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS)
            .map_err(|_| OutOfBounds)?;

        let slot = self.0.chunks.get_mut(idx).ok_or(OutOfBounds)?;
        *slot = Some(handle);

        Ok(())
    }

    pub fn build(self) -> Neighbors<'a> {
        self.0
    }
}
