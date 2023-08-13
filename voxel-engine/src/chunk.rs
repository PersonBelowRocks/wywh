use bevy::prelude::*;

use crate::{error::ChunkVoxelError, tile::VoxelId, util};

const CHUNK_SIZE: usize = 16;

pub struct ChunkVoxelData<T>(pub(crate) [[[T; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE]);

impl<T: Copy> ChunkVoxelData<T> {
    pub fn new(filling: T) -> Self {
        Self([[[filling; CHUNK_SIZE]; CHUNK_SIZE]; CHUNK_SIZE])
    }
}

pub struct Chunk {
    voxel_data: Option<Box<ChunkVoxelData<VoxelId>>>,
}

#[allow(dead_code)]
impl Chunk {
    pub const SIZE: usize = CHUNK_SIZE;

    pub fn new(voxel_data: ChunkVoxelData<VoxelId>) -> Self {
        Self {
            voxel_data: Some(Box::new(voxel_data)),
        }
    }

    #[inline]
    pub fn try_get_voxel(&self, pos: IVec3) -> Result<&VoxelId, ChunkVoxelError> {
        let [x, y, z] = util::try_ivec3_to_usize_arr(pos)?;
        self.voxel_data
            .as_ref()
            .ok_or(ChunkVoxelError::NotInitializedError)
            .map(|cvd| cvd.0.get(x)?.get(y)?.get(z))
            .and_then(|opt| opt.ok_or(ChunkVoxelError::OutOfBounds))
    }

    #[inline]
    pub fn get_voxel(&self, pos: IVec3) -> Option<&VoxelId> {
        self.try_get_voxel(pos).ok()
    }

    /// Returns [`Result::Ok`] if the set operation was successful.
    /// Returns [`Result::Err`]`(`[`ChunkVoxelError`]`)` if something went wrong, the [`ChunkVoxelError`] describes what went wrong.
    #[inline]
    pub fn try_set_voxel(&mut self, pos: IVec3, vox: VoxelId) -> Result<(), ChunkVoxelError> {
        let [x, y, z] = util::try_ivec3_to_usize_arr(pos)?;

        let slot = self
            .voxel_data
            .as_mut()
            .ok_or(ChunkVoxelError::NotInitializedError)
            .map(|cvd| cvd.0.get_mut(x)?.get_mut(y)?.get_mut(z))
            .and_then(|opt| opt.ok_or(ChunkVoxelError::OutOfBounds))?;

        *slot = vox;

        Ok(())
    }

    #[inline]
    pub fn set_voxel(&mut self, pos: IVec3, vox: VoxelId) {
        #[allow(unused_must_use)]
        {
            self.try_set_voxel(pos, vox);
        }
    }
}
