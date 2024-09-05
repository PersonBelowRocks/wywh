use bevy::math::{ivec3, IVec2, IVec3};

use crate::{
    data::{registries::block::BlockVariantId, tile::Face},
    topo::{bounding_box::BoundingBox, ivec_project_to_3d},
    util::ivec3_to_1d,
};

use super::{
    error::NeighborReadError,
    fb_localspace_to_local_chunkspace, fb_localspace_wrap, mb_localspace_to_local_chunkspace,
    mb_localspace_wrap,
    world::{chunk::ChunkReadHandle, Chunk, OutOfBounds},
    CHUNK_MICROBLOCK_DIMS, FULL_BLOCK_MICROBLOCK_DIMS,
};

fn local_fb_to_chunk_pos(pos: IVec3) -> IVec3 {
    // TODO: use bitwise math
    ivec3(
        pos.x.div_euclid(Chunk::SIZE),
        pos.y.div_euclid(Chunk::SIZE),
        pos.z.div_euclid(Chunk::SIZE),
    )
}

fn local_fb_to_neighbor_local_fb(pos: IVec3) -> IVec3 {
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

/// Test if the provided microblock facespace vector is in bounds
pub fn is_in_bounds_mb(pos: IVec2) -> bool {
    let min: IVec2 = -IVec2::splat(FULL_BLOCK_MICROBLOCK_DIMS as _);
    let max: IVec2 =
        IVec2::splat(CHUNK_MICROBLOCK_DIMS as _) + IVec2::splat(FULL_BLOCK_MICROBLOCK_DIMS as _);

    pos.cmpge(min).all() && pos.cmplt(max).all()
}

/// Test if the provided localspace vector is in bounds
pub fn is_in_bounds_3d(pos: IVec3) -> bool {
    let min: IVec3 = -IVec3::ONE;
    let max: IVec3 = IVec3::splat(Chunk::SIZE) + IVec3::ONE;

    pos.cmpge(min).all() && pos.cmplt(max).all() && local_fb_to_chunk_pos(pos) != IVec3::ZERO
}

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

    /// Get a handle to a neighboring chunk from a local chunk position. Returns [`NeighborReadError::OutOfBounds`]
    /// if the given chunk position is either `[0, 0, 0]` or not inclusively between `[-1, -1, -1]..[1, 1, 1]`.
    ///
    /// # Vectors
    /// `chunk_pos` is in local, neighbor-only, chunk space
    #[inline]
    pub fn get_neighbor_chunk(
        &self,
        chunk_pos: IVec3,
    ) -> Result<Option<&ChunkReadHandle<'_>>, NeighborReadError> {
        if chunk_pos == IVec3::ZERO {
            // tried to access center chunk (aka. the chunk for which we represent the neighbors)
            return Err(NeighborReadError::OutOfBounds);
        }

        let chunk_index = ivec3_to_1d(chunk_pos + IVec3::ONE, NEIGHBOR_CUBIC_ARRAY_DIMENSIONS)
            .map_err(|_| NeighborReadError::OutOfBounds)?;
        let chunk = self
            .chunks
            .get(chunk_index)
            .ok_or(NeighborReadError::OutOfBounds)?;

        Ok(chunk.as_ref())
    }

    /// Get a block in one of the neighboring chunks, returning the default block if there was no handle
    /// for that chunk. This function allows reading from all blocks in the neighboring chunks, not just
    /// the ones on the borders facing the center.
    /// # Vectors
    /// `ls_nb_pos` is chunk local, full-block, and neighbor-only
    #[inline]
    pub fn get_3d(&self, ls_nb_pos: IVec3) -> Result<Option<BlockVariantId>, NeighborReadError> {
        let chunk_pos = fb_localspace_to_local_chunkspace(ls_nb_pos);

        let chunk = self.get_neighbor_chunk(chunk_pos)?;

        chunk
            .map(|handle| {
                // Wrap the localspace position around since it refers to a position in a
                // neighboring chunk
                let neighbor_local = fb_localspace_wrap(ls_nb_pos);
                handle.get(neighbor_local).map_err(NeighborReadError::from)
            })
            .unwrap_or(Ok(Some(self.default_block)))
    }

    /// Get a microblock in one of the neighboring chunks, returning the default block if there was no handle
    /// for that chunk. This function allows reading from all microblocks in the neighboring chunks, not just
    /// the ones on the borders facing the center.
    /// # Vectors
    /// `mb_nb_pos` is chunk local, microblock, and neighbor-only
    #[inline]
    pub fn get_3d_mb(&self, mb_nb_pos: IVec3) -> Result<BlockVariantId, NeighborReadError> {
        let chunk_pos = mb_localspace_to_local_chunkspace(mb_nb_pos);

        let chunk = self.get_neighbor_chunk(chunk_pos)?;

        chunk
            .map(|handle| {
                // Wrap the localspace position around since it refers to a position in a
                // neighboring chunk
                let neighbor_local = mb_localspace_wrap(mb_nb_pos);
                handle
                    .get_mb(neighbor_local)
                    .map_err(NeighborReadError::from)
            })
            .unwrap_or(Ok(self.default_block))
    }

    /// Get the block in a neighboring chunk that "obscures" the given block position in the center chunk.
    /// The position may exceed the chunks borders by 1 to allow getting blocks diagonal of the center chunk.
    ///
    /// # Vectors
    /// `face_pos` is chunk local, full-block, and on face
    #[inline]
    pub fn get(
        &self,
        face: Face,
        face_pos: IVec2,
    ) -> Result<Option<BlockVariantId>, NeighborReadError> {
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

    /// Get the microblock in a neighboring chunk that "obscures" the given microblock position in the center chunk.
    /// The position may exceed the chunks borders by 1 to allow getting blocks diagonal of the center chunk.
    ///
    /// # Vectors
    /// `mb_face_pos` is chunk local, microblock, and on face
    #[inline]
    pub fn get_mb(
        &self,
        face: Face,
        mb_face_pos: IVec2,
    ) -> Result<BlockVariantId, NeighborReadError> {
        if !is_in_bounds_mb(mb_face_pos) {
            return Err(NeighborReadError::OutOfBounds);
        }

        let mb_pos_3d = {
            let mut mag = face.axis_direction();
            if mag > 0 {
                mag = Chunk::SIZE;
            }

            ivec_project_to_3d(mb_face_pos, face, mag)
        };

        self.get_3d_mb(mb_pos_3d)
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
