use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Weak,
};

use bevy::prelude::IVec3;

use crate::data::tile::VoxelId;

use super::{
    access::{HasBounds, ReadAccess, WriteAccess},
    bounding_box::BoundingBox,
    chunk::{Chunk, ChunkPos},
    containers::{SyncChunkVoxelContainerAccess, SyncChunkVoxelContainerReadAccess},
    error::{ChunkRefAccessError, ChunkVoxelAccessError},
};

#[derive(Clone)]
pub struct ChunkRef {
    pub(crate) chunk: Weak<Chunk>,
    pub(crate) changed: Weak<AtomicBool>,
    pub(crate) pos: ChunkPos,
}

impl ChunkRef {
    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    pub fn treat_as_changed(&self) -> Result<(), ChunkRefAccessError> {
        let changed = self
            .changed
            .upgrade()
            .ok_or(ChunkRefAccessError::Unloaded)?;
        changed.store(true, Ordering::SeqCst);
        Ok(())
    }

    #[allow(clippy::let_and_return)] // We need do to this little crime so the borrowchecker doesn't yell at us
    pub fn with_access<F, U>(&self, f: F) -> Result<U, ChunkRefAccessError>
    where
        F: for<'a> FnOnce(ChunkRefVxlAccess<'a>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkRefAccessError::Unloaded)?;
        self.treat_as_changed()?;

        let internal_access = chunk.voxels.access();

        let x = Ok(f(ChunkRefVxlAccess(internal_access)));
        x
    }

    #[allow(clippy::let_and_return)]
    pub fn with_read_access<F, U>(&self, f: F) -> Result<U, ChunkRefAccessError>
    where
        F: for<'a> FnOnce(ChunkRefVxlReadAccess<'a>) -> U,
    {
        let chunk = self.chunk.upgrade().ok_or(ChunkRefAccessError::Unloaded)?;

        let internal_access = chunk.voxels.read_access();

        let x = Ok(f(ChunkRefVxlReadAccess(internal_access)));
        x
    }
}

pub struct ChunkRefVxlReadAccess<'a>(SyncChunkVoxelContainerReadAccess<'a, VoxelId>);

pub struct ChunkRefVxlAccess<'a>(SyncChunkVoxelContainerAccess<'a, VoxelId>);

impl<'a> WriteAccess for ChunkRefVxlAccess<'a> {
    type WriteErr = ChunkVoxelAccessError;
    type WriteType = VoxelId;

    fn set(&mut self, pos: IVec3, data: Self::WriteType) -> Result<(), Self::WriteErr> {
        self.0.set(pos, data)
    }
}

impl<'a> ReadAccess for ChunkRefVxlAccess<'a> {
    type ReadErr = ChunkVoxelAccessError;
    type ReadType = VoxelId;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        self.0.get(pos)
    }
}

impl<'a> HasBounds for ChunkRefVxlAccess<'a> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}

impl<'a> ReadAccess for ChunkRefVxlReadAccess<'a> {
    type ReadErr = ChunkVoxelAccessError;
    type ReadType = VoxelId;

    fn get(&self, pos: IVec3) -> Result<Self::ReadType, Self::ReadErr> {
        self.0.get(pos)
    }
}

impl<'a> HasBounds for ChunkRefVxlReadAccess<'a> {
    fn bounds(&self) -> BoundingBox {
        Chunk::BOUNDING_BOX
    }
}
